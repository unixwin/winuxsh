//! Shell state and execution entry point
//!
//! Wraps a `rubash::Executor` and provides the interactive shell machinery
//! (prompt, history, completion). All shell language semantics are delegated
//! to rubash; this layer only adds the Windows-facing UX.


use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reedline::Reedline;
use rubash::{executor::Executor, lexer::tokenize, parser::parse};

use crate::completion::runtime::RuntimeCompletionPlugin;
use crate::completion::CompletionState;
use crate::config::{
    load as load_config, AutosuggestConfig, EditorMode, HookConfig, NativePluginConfig,
    NativeWidgetConfig, SyntaxHighlightConfig,
};
use crate::prompt::WinuxshPrompt;
use crate::zsh_compat::{
    apply_alias, apply_safe_env, completion_defs_from_report,
    dynamic_completion_defs_from_report_with_options, git_prompt_format_from_report, scan,
    runtime_completion_commands_from_report, DynamicCompletionRunOptions, NativeWidgetSuggestion,
    ZshImportOptions,
};

use crate::winuxcmd;

/// Top-level shell state.
pub struct Shell {
    pub executor: Executor,
    pub completion_state: Arc<Mutex<CompletionState>>,
    pub prompt: WinuxshPrompt,
    pub history_path: std::path::PathBuf,
    pub editor_mode: EditorMode,
    pub autosuggest: AutosuggestConfig,
    pub syntax_highlighting: SyntaxHighlightConfig,
    pub native_widgets: NativeWidgetConfig,
    pub native_widget_bindings: Vec<NativeWidgetSuggestion>,
    pub native_plugins: NativePluginConfig,
    pub hooks: HookConfig,
    pub aliases: HashMap<String, String>,
    pub zoxide_last_tracked_dir: Option<String>,
    pub line_editor: Option<Reedline>,
}

impl Shell {
    /// Construct a fresh shell: load config, install Ctrl+C handler, inject
    /// winuxcmd onto PATH, set up completion state and history.
    pub fn new() -> anyhow::Result<Self> {
        // 1. Load config from ~/.winshrc.toml.
        let config = load_config();

        // 2. Apply opt-in, known-safe zsh profile env/PATH records before
        // winuxcmd injection and before rubash snapshots the process env.
        let zsh_report = if config.zsh.enabled && config.zsh.auto_apply {
            let report = scan(&ZshImportOptions::from_config(&config.zsh));
            let summary = apply_safe_env(&report);
            log::debug!(
                "zsh safe env import: env={} path_entries={}",
                summary.env_applied,
                summary.path_entries_applied
            );
            Some(report)
        } else {
            None
        };

        // 3. WinuxCmd PATH injection (best-effort), honoring config override.
        if let Err(e) = winuxcmd::ensure_on_path_with_override(config.winuxcmd_path.as_deref()) {
            log::warn!("winuxcmd not on PATH: {}", e);
        }

        // 4. Build rubash Executor after PATH injection.
        let mut executor = Executor::new();

        // 5. Apply imported aliases first, then native config aliases so
        // ~/.winshrc.toml remains authoritative when names collide.
        let mut aliases = HashMap::new();
        if let Some(report) = &zsh_report {
            let mut aliases_applied = 0usize;
            for alias in &report.aliases {
                if apply_alias(&mut executor, &alias.name, &alias.value) {
                    aliases.insert(alias.name.clone(), alias.value.clone());
                    aliases_applied += 1;
                }
            }
            log::debug!("zsh safe alias import: aliases={}", aliases_applied);
        }
        for (name, value) in &config.aliases {
            if apply_alias(&mut executor, name, value) {
                aliases.insert(name.clone(), value.clone());
            } else {
                log::warn!("Skipping invalid alias from config: {}", name);
            }
        }

        // 6. Prompt + theme. Native TOML stays authoritative; zsh prompt
        // imports only fill empty native prompt fields.
        let prompt_format = config.shell.prompt_format.clone().or_else(|| {
            zsh_report.as_ref().and_then(|report| {
                report
                    .prompt
                    .as_ref()
                    .and_then(|prompt| prompt.translated_format.clone())
            })
        });
        let right_prompt_format = config.shell.right_prompt_format.clone().or_else(|| {
            zsh_report.as_ref().and_then(|report| {
                report
                    .right_prompt
                    .as_ref()
                    .and_then(|prompt| prompt.translated_format.clone())
            })
        });
        let git_prompt_format = zsh_report
            .as_ref()
            .and_then(git_prompt_format_from_report);
        let prompt = WinuxshPrompt::new(
            prompt_format,
            right_prompt_format,
            git_prompt_format,
            &config.theme_name,
        );

        // 7. History file in home dir.
        let history_path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".winuxsh_history");

        // 8. Completion state.
        let completion_state = Arc::new(Mutex::new(CompletionState::new(
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        )));
        let mut zsh_completion_defs = zsh_report
            .as_ref()
            .map(completion_defs_from_report)
            .unwrap_or_default();
        if let (Some(report), Some(options)) = (
            zsh_report.as_ref(),
            DynamicCompletionRunOptions::from_zsh_config(&config.zsh),
        ) {
            zsh_completion_defs.extend(dynamic_completion_defs_from_report_with_options(
                report,
                &options,
            ));
        }

        // 9. Load completion dirs from config (inline, not in thread).
        {
            let mut s = completion_state.lock().unwrap();
            s.load_completion_dirs_with_definitions(&config.completion_dirs, zsh_completion_defs);
            if config.zsh.runtime_completions.enabled {
                if let Some(report) = zsh_report.as_ref() {
                    let runtime_commands = runtime_completion_commands_from_report(
                        report,
                        &config.zsh.runtime_completions.commands,
                    );
                    if !runtime_commands.is_empty() {
                        s.add_plugin(Arc::new(RuntimeCompletionPlugin::new(
                            runtime_commands,
                            Duration::from_millis(
                                config.zsh.runtime_completions.timeout_millis.max(1),
                            ),
                        )));
                    }
                }
            }
        }

        let mut syntax_highlighting = config.zsh.syntax_highlighting.clone();
        if let Some(report) = &zsh_report {
            for style in &report.highlight_styles {
                syntax_highlighting
                    .styles
                    .entry(style.key.clone())
                    .or_insert_with(|| style.value.clone());
            }
        }
        let native_widget_bindings = if config.zsh.native_widgets.enabled
            && config.zsh.native_widgets.import_bindkeys
        {
            zsh_report
                .as_ref()
                .map(|report| report.native_widgets.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(Self {
            executor,
            completion_state,
            prompt,
            history_path,
            editor_mode: config.editor.edit_mode,
            autosuggest: config.zsh.autosuggestions.with_env_overrides(),
            syntax_highlighting: syntax_highlighting.with_env_overrides(),
            native_widgets: config.zsh.native_widgets,
            native_widget_bindings,
            native_plugins: config.zsh.native_plugins,
            hooks: config.hooks,
            aliases,
            zoxide_last_tracked_dir: None,
            line_editor: None,
        })
    }

    /// Execute a single input line via rubash. Returns the exit code.
    pub fn execute_line(&mut self, line: &str) -> anyhow::Result<i32> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(0);
        }

        let tokens = tokenize(line);
        if tokens.is_empty() {
            return Ok(0);
        }

        // parse() returns Ast directly (not Result) in rubash.
        let ast = parse(&tokens);

        if self.native_plugin_enabled("zoxide")
            && ast.commands.len() == 1
            && ast.commands[0]
                .words
                .first()
                .is_some_and(|command| command == "z")
        {
            return self.execute_native_zoxide(&ast.commands[0].words[1..]);
        }

        match self.executor.execute_ast(&ast) {
            Ok(()) => {}
            Err(rubash::executor::ExecuteError::ExitCode(code)) => {
                return Ok(code);
            }
            Err(rubash::executor::ExecuteError::Return(code)) => {
                return Ok(code);
            }
            Err(rubash::executor::ExecuteError::CommandNotFound(cmd)) => {
                eprintln!("winuxsh: {}: command not found", cmd);
                return Ok(127);
            }
            Err(e) => {
                eprintln!("winuxsh: {}", e);
                return Ok(1);
            }
        }

        Ok(self.executor.last_exit_code())
    }

    /// Execute a line as an interactive REPL command, including native hook
    /// points that mirror common zsh lifecycle concepts.
    pub fn execute_interactive_line(&mut self, line: &str) -> anyhow::Result<i32> {
        let old_pwd = self.executor.get_env("PWD").map(str::to_owned);
        self.run_preexec_hooks(line);
        let code = self.execute_line(line)?;
        self.sync_alias_mirror_from_line(line, code);
        let new_pwd = self.executor.get_env("PWD").map(str::to_owned);
        if let (Some(old_pwd), Some(new_pwd)) = (old_pwd, new_pwd) {
            self.run_chpwd_hooks_if_changed(&old_pwd, &new_pwd);
        }
        self.update_completion_state();
        Ok(code)
    }

    /// Run native hooks before rendering the next prompt.
    pub fn run_precmd_hooks(&mut self) {
        self.run_native_precmd_plugins();
        let hooks = self.hooks.precmd.clone();
        let last_exit_code = self.executor.last_exit_code().to_string();
        self.run_hook_scripts(&hooks, &[("WINUXSH_LAST_EXIT_CODE", last_exit_code)]);
    }

    /// Run native hooks immediately before the user's interactive command.
    pub fn run_preexec_hooks(&mut self, command: &str) {
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        self.run_native_preexec_plugins(command);
        let hooks = self.hooks.preexec.clone();
        self.run_hook_scripts(&hooks, &[("WINUXSH_PREEXEC_COMMAND", command.to_string())]);
    }

    /// Run native hooks when the interactive command changed directories.
    pub fn run_chpwd_hooks_if_changed(&mut self, old_pwd: &str, new_pwd: &str) {
        if same_shell_dir(old_pwd, new_pwd) {
            return;
        }
        self.run_native_chpwd_plugins();
        let hooks = self.hooks.chpwd.clone();
        self.run_hook_scripts(
            &hooks,
            &[
                ("WINUXSH_OLDPWD", old_pwd.to_string()),
                ("WINUXSH_PWD", new_pwd.to_string()),
            ],
        );
    }

    fn run_native_precmd_plugins(&mut self) {
        if self.native_plugin_enabled("direnv") {
            self.apply_direnv_export();
        }
        if self.native_plugin_enabled("zoxide") {
            self.track_zoxide_current_dir();
        }
    }

    fn run_native_preexec_plugins(&mut self, command: &str) {
        if self.native_plugin_enabled("alias-finder") {
            for suggestion in self.native_alias_finder_matches(command) {
                println!("{}", suggestion);
            }
        }
    }

    fn run_native_chpwd_plugins(&mut self) {
        if self.native_plugin_enabled("direnv") {
            self.apply_direnv_export();
        }
        if self.native_plugin_enabled("zoxide") {
            self.track_zoxide_current_dir();
        }
    }

    fn native_plugin_enabled(&self, preset: &str) -> bool {
        self.native_plugins.enabled
            && self
                .native_plugins
                .presets
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(preset))
    }

    fn apply_direnv_export(&mut self) {
        let command_path =
            resolve_native_command_path("direnv").unwrap_or_else(|| PathBuf::from("direnv"));
        let output = match Command::new(command_path)
            .args(["export", "bash"])
            .stderr(Stdio::null())
            .output()
        {
            Ok(output) => output,
            Err(err) => {
                log::debug!("native direnv preset skipped: {}", err);
                return;
            }
        };

        if !output.status.success() {
            log::debug!("native direnv preset returned {}", output.status);
            return;
        }

        let script = String::from_utf8_lossy(&output.stdout);
        self.apply_direnv_export_script(&script);
    }

    fn apply_direnv_export_script(&mut self, script: &str) {
        if script.trim().is_empty() {
            return;
        }
        if let Err(err) = self.execute_script(script) {
            log::warn!("native direnv preset failed to apply export: {}", err);
        }
    }

    fn track_zoxide_current_dir(&mut self) {
        let Some(pwd) = self.executor.get_env("PWD").map(str::to_owned) else {
            return;
        };
        if self
            .zoxide_last_tracked_dir
            .as_deref()
            .is_some_and(|last| same_shell_dir(last, &pwd))
        {
            return;
        }

        let host_pwd = shell_path_to_host_path(&pwd);
        let command_path =
            resolve_native_command_path("zoxide").unwrap_or_else(|| PathBuf::from("zoxide"));
        let status = Command::new(command_path)
            .arg("add")
            .arg(&host_pwd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match status {
            Ok(status) if status.success() => {
                self.zoxide_last_tracked_dir = Some(pwd);
            }
            Ok(status) => {
                log::debug!("native zoxide preset returned {}", status);
            }
            Err(err) => {
                log::debug!("native zoxide preset skipped: {}", err);
            }
        }
    }

    fn execute_native_zoxide(&mut self, args: &[String]) -> anyhow::Result<i32> {
        let command_path =
            resolve_native_command_path("zoxide").unwrap_or_else(|| PathBuf::from("zoxide"));
        let output = match Command::new(command_path)
            .arg("query")
            .args(args)
            .stderr(Stdio::null())
            .output()
        {
            Ok(output) => output,
            Err(err) => {
                log::debug!("native zoxide query skipped: {}", err);
                return Ok(127);
            }
        };

        if !output.status.success() {
            return Ok(output.status.code().unwrap_or(1));
        }

        let target = String::from_utf8_lossy(&output.stdout);
        let target = target.trim_matches(['\r', '\n']);
        if target.is_empty() {
            return Ok(1);
        }

        let target = host_path_to_shell_path(target);
        self.execute_line(&format!("cd {}", shell_quote(&target)))
    }

    fn native_alias_finder_matches(&self, command: &str) -> Vec<String> {
        let command = normalize_alias_finder_command(command);
        if command.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<_> = self
            .aliases
            .iter()
            .filter_map(|(name, value)| {
                if normalize_alias_finder_command(value) == command && name != &command {
                    Some(format!("winuxsh: alias available: {}={}", name, shell_quote(value)))
                } else {
                    None
                }
            })
            .collect();
        matches.sort();
        matches
    }

    fn sync_alias_mirror_from_line(&mut self, line: &str, code: i32) {
        if code != 0 {
            return;
        }

        let tokens = tokenize(line);
        if tokens.is_empty() {
            return;
        }

        let ast = parse(&tokens);
        if ast.commands.len() != 1 {
            return;
        }

        let mut words = ast.commands[0].words.as_slice();
        if words.first().is_some_and(|word| word == "builtin") {
            words = &words[1..];
        }

        match words.first().map(String::as_str) {
            Some("alias") => self.sync_alias_assignments(&words[1..]),
            Some("unalias") => self.sync_unalias_arguments(&words[1..]),
            _ => {}
        }
    }

    fn sync_alias_assignments(&mut self, args: &[String]) {
        for arg in args {
            if arg == "-p" || arg == "--" {
                continue;
            }
            let Some((name, value)) = arg.split_once('=') else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            self.aliases
                .insert(name.to_string(), strip_rubash_alias_quote_marker(value).to_string());
        }
    }

    fn sync_unalias_arguments(&mut self, args: &[String]) {
        let mut allow_options = true;
        for arg in args {
            if allow_options && arg == "--" {
                allow_options = false;
                continue;
            }
            if allow_options && arg == "-a" {
                self.aliases.clear();
                continue;
            }
            if allow_options && arg.starts_with('-') {
                continue;
            }
            self.aliases.remove(arg);
        }
    }

    fn run_hook_scripts(&mut self, hooks: &[String], context: &[(&str, String)]) {
        if hooks.is_empty() {
            return;
        }

        for (name, value) in context {
            self.executor.set_env(name, value);
        }

        for hook in hooks {
            match self.execute_script(hook) {
                Ok(0) => {}
                Ok(code) => log::warn!("native hook exited with status {}", code),
                Err(err) => log::warn!("native hook failed: {}", err),
            }
        }

        if !context.is_empty() {
            let unset = format!(
                "unset {}",
                context
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            let _ = self.execute_script(&unset);
        }
    }

    /// Update the shared completion state from the current env + cwd.
    pub fn update_completion_state(&self) {
        if let Ok(mut state) = self.completion_state.lock() {
            state.current_dir = std::env::current_dir().unwrap_or_else(|_| state.current_dir.clone());
            state.env_vars = std::env::vars()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        }
    }

    /// Last exit code from rubash executor.
    pub fn last_exit_code(&self) -> i32 {
        self.executor.last_exit_code()
    }

    /// Execute an entire script (multi-line) via rubash full AST execution.
    ///
    /// Unlike `execute_line` which tokenizes/parses/executes each line
    /// independently, this method tokenizes the whole script at once.
    /// This enables heredocs, line continuations (backslash-newline),
    /// and multi-line compound commands (if/for/while across lines).
    pub fn execute_script(&mut self, script: &str) -> anyhow::Result<i32> {
        let script = script.trim();
        if script.is_empty() {
            return Ok(0);
        }

        let tokens = tokenize(script);
        if tokens.is_empty() {
            return Ok(0);
        }

        let ast = parse(&tokens);

        match self.executor.execute_ast(&ast) {
            Ok(()) => {}
            Err(rubash::executor::ExecuteError::ExitCode(code)) => {
                return Ok(code);
            }
            Err(rubash::executor::ExecuteError::Return(code)) => {
                return Ok(code);
            }
            Err(rubash::executor::ExecuteError::CommandNotFound(cmd)) => {
                eprintln!("winuxsh: {}: command not found", cmd);
                return Ok(127);
            }
            Err(e) => {
                eprintln!("winuxsh: {}", e);
                return Ok(1);
            }
        }

        Ok(self.executor.last_exit_code())
    }
}

fn same_shell_dir(left: &str, right: &str) -> bool {
    let left = left.trim_end_matches(['/', '\\']).replace('/', "\\");
    let right = right.trim_end_matches(['/', '\\']).replace('/', "\\");
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn normalize_alias_finder_command(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_rubash_alias_quote_marker(value: &str) -> &str {
    value.strip_prefix('\x1c').unwrap_or(value)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn resolve_native_command_path(command: &str) -> Option<PathBuf> {
    let command_path = PathBuf::from(command);
    if command_path.is_file() {
        return Some(command_path);
    }

    let path = std::env::var_os("PATH")?;
    let has_extension = PathBuf::from(command)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some();
    let extensions: &[&str] = if has_extension {
        &[""]
    } else if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };

    for dir in std::env::split_paths(&path) {
        for ext in extensions {
            let candidate = dir.join(format!("{}{}", command, ext));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn shell_path_to_host_path(value: &str) -> String {
    let normalized = value.replace('\\', "/");
    if cfg!(windows) {
        let bytes = normalized.as_bytes();
        if bytes.len() == 2
            && bytes[0] == b'/'
            && (bytes[1] as char).is_ascii_alphabetic()
        {
            let drive = (bytes[1] as char).to_ascii_uppercase();
            return format!("{}:/", drive);
        }
        if bytes.len() >= 3
            && bytes[0] == b'/'
            && (bytes[1] as char).is_ascii_alphabetic()
            && bytes[2] == b'/'
        {
            let drive = (bytes[1] as char).to_ascii_uppercase();
            return format!("{}:{}", drive, &normalized[2..]);
        }
    }
    value.to_string()
}

fn host_path_to_shell_path(value: &str) -> String {
    if cfg!(windows) {
        let normalized = value.replace('\\', "/");
        let bytes = normalized.as_bytes();
        if bytes.len() >= 3
            && bytes[1] == b':'
            && bytes[0].is_ascii_alphabetic()
            && bytes[2] == b'/'
        {
            let drive = (bytes[0] as char).to_ascii_lowercase();
            return format!("/{drive}/{}", &normalized[3..]);
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn native_lifecycle_hooks_run_for_interactive_commands() {
        let temp = unique_temp_dir("winuxsh-native-hooks");
        let next_dir = temp.join("next");
        std::fs::create_dir_all(&next_dir).unwrap();
        let next_arg = shell_quote(&shell_display_path(&next_dir));

        let mut shell = test_shell(HookConfig {
            precmd: vec!["HOOK_PRECMD=\"precmd:$WINUXSH_LAST_EXIT_CODE\"".to_string()],
            preexec: vec!["HOOK_PREEXEC=\"preexec:$WINUXSH_PREEXEC_COMMAND\"".to_string()],
            chpwd: vec!["HOOK_CHPWD=\"chpwd:$WINUXSH_OLDPWD->$WINUXSH_PWD\"".to_string()],
        });

        shell.run_precmd_hooks();
        shell
            .execute_interactive_line(&format!("cd {}", next_arg))
            .unwrap();

        assert_eq!(shell.executor.get_env("HOOK_PRECMD"), Some("precmd:0"));
        let preexec = shell.executor.get_env("HOOK_PREEXEC").unwrap_or_default();
        assert!(preexec.starts_with("preexec:cd "), "{preexec}");
        let chpwd = shell.executor.get_env("HOOK_CHPWD").unwrap_or_default();
        assert!(chpwd.starts_with("chpwd:"), "{chpwd}");
        assert!(chpwd.contains("->"), "{chpwd}");
        assert!(shell.executor.get_env("WINUXSH_LAST_EXIT_CODE").is_none());
        assert!(shell.executor.get_env("WINUXSH_PREEXEC_COMMAND").is_none());
        assert!(shell.executor.get_env("WINUXSH_OLDPWD").is_none());
        assert!(shell.executor.get_env("WINUXSH_PWD").is_none());

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn native_direnv_export_script_applies_to_executor_env() {
        let mut shell = test_shell(HookConfig::default());

        shell.apply_direnv_export_script("export DIRENV_TEST_VALUE=active\n");

        assert_eq!(shell.executor.get_env("DIRENV_TEST_VALUE"), Some("active"));
    }

    #[test]
    fn native_alias_finder_matches_known_alias_values() {
        let mut shell = test_shell(HookConfig::default());
        shell.native_plugins.enabled = true;
        shell.native_plugins.presets = vec!["alias-finder".to_string()];
        shell
            .aliases
            .insert("gst".to_string(), "git status".to_string());

        assert_eq!(
            shell.native_alias_finder_matches(" git   status "),
            vec!["winuxsh: alias available: gst='git status'"]
        );
        assert!(shell.native_alias_finder_matches("git diff").is_empty());
    }

    #[test]
    fn alias_mirror_tracks_successful_interactive_alias_commands() {
        let mut shell = test_shell(HookConfig::default());

        shell.execute_interactive_line("alias gst='git status'").unwrap();
        assert_eq!(shell.aliases.get("gst").map(String::as_str), Some("git status"));
        assert_eq!(
            shell.native_alias_finder_matches("git status"),
            vec!["winuxsh: alias available: gst='git status'"]
        );

        shell.execute_interactive_line("unalias gst").unwrap();
        assert!(shell.native_alias_finder_matches("git status").is_empty());
    }

    #[test]
    fn shell_path_to_host_path_converts_drive_style_paths() {
        if cfg!(windows) {
            assert_eq!(shell_path_to_host_path("/c/Users/me/project"), "C:/Users/me/project");
            assert_eq!(shell_path_to_host_path("/d"), "D:/");
        } else {
            assert_eq!(shell_path_to_host_path("/c/Users/me/project"), "/c/Users/me/project");
        }
    }

    #[test]
    fn native_zoxide_command_changes_directory_and_tracks_pwd() {
        let temp = unique_temp_dir("winuxsh-native-zoxide");
        let bin = temp.join("bin");
        let target = temp.join("target");
        let log = temp.join("zoxide-add.txt");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&target).unwrap();

        let target_path = host_display_path(&target);
        let target_shell_path = shell_display_path(&target);
        let log_path = host_display_path(&log);
        write_fake_zoxide(&bin, &target_path, &log_path);
        let old_path = prepend_path_for_test(&bin);

        let mut shell = test_shell(HookConfig::default());
        shell.native_plugins.enabled = true;
        shell.native_plugins.presets = vec!["zoxide".to_string()];

        shell.execute_line("z project").unwrap();
        let pwd = shell.executor.get_env("PWD").unwrap_or_default();
        assert!(
            same_shell_dir(&pwd, &target_shell_path),
            "{pwd} != {target_shell_path}"
        );

        shell.run_precmd_hooks();
        let tracked = std::fs::read_to_string(&log).unwrap();
        assert_eq!(
            tracked.trim(),
            shell_path_to_host_path(shell.executor.get_env("PWD").unwrap_or_default())
        );

        restore_path_for_test(old_path);
        let _ = std::fs::remove_dir_all(temp);
    }

    fn test_shell(hooks: HookConfig) -> Shell {
        Shell {
            executor: Executor::new(),
            completion_state: Arc::new(Mutex::new(CompletionState::new(PathBuf::from(".")))),
            prompt: WinuxshPrompt::new(None, None, None, "default"),
            history_path: PathBuf::from(".winuxsh_history"),
            editor_mode: EditorMode::Emacs,
            autosuggest: AutosuggestConfig::default(),
            syntax_highlighting: SyntaxHighlightConfig::default(),
            native_widgets: NativeWidgetConfig::default(),
            native_widget_bindings: Vec::new(),
            native_plugins: NativePluginConfig::default(),
            hooks,
            aliases: HashMap::new(),
            zoxide_last_tracked_dir: None,
            line_editor: None,
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
    }

    fn shell_display_path(path: &std::path::Path) -> String {
        let mut value = path.to_string_lossy().replace('\\', "/");
        if cfg!(windows)
            && value.len() >= 3
            && value.as_bytes()[1] == b':'
            && value.as_bytes()[2] == b'/'
        {
            let drive = value.as_bytes()[0] as char;
            value = format!("/{}{}", drive.to_ascii_lowercase(), &value[2..]);
        }
        value
    }

    fn host_display_path(path: &std::path::Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn prepend_path_for_test(dir: &std::path::Path) -> Option<std::ffi::OsString> {
        let old_path = std::env::var_os("PATH");
        let mut paths = vec![dir.to_path_buf()];
        if let Some(old_path) = &old_path {
            paths.extend(std::env::split_paths(old_path));
        }
        let new_path = std::env::join_paths(paths).unwrap();
        std::env::set_var("PATH", new_path);
        old_path
    }

    fn restore_path_for_test(old_path: Option<std::ffi::OsString>) {
        match old_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }

    fn write_fake_zoxide(bin: &std::path::Path, target_path: &str, log_path: &str) {
        let script = if cfg!(windows) {
            format!(
                "@echo off\r\nif \"%1\"==\"query\" (\r\n  <nul set /p ={}\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"add\" (\r\n  >\"{}\" echo %~2\r\n  exit /b 0\r\n)\r\nexit /b 1\r\n",
                target_path, log_path
            )
        } else {
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"query\" ]; then\n  printf '%s\\n' '{}'\n  exit 0\nfi\nif [ \"$1\" = \"add\" ]; then\n  printf '%s\\n' \"$2\" > '{}'\n  exit 0\nfi\nexit 1\n",
                target_path, log_path
            )
        };
        let exe = bin.join(if cfg!(windows) { "zoxide.cmd" } else { "zoxide" });
        std::fs::write(&exe, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&exe).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&exe, permissions).unwrap();
        }
    }
}
