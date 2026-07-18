//! Shell state and execution entry point
//!
//! Wraps a `rubash::Executor` and provides the interactive shell machinery
//! (prompt, history, completion). All shell language semantics are delegated
//! to rubash; this layer only adds the Windows-facing UX.


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
    apply_alias, apply_safe_aliases, apply_safe_env, completion_defs_from_report,
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
        if let Some(report) = &zsh_report {
            let summary = apply_safe_aliases(report, &mut executor);
            log::debug!("zsh safe alias import: aliases={}", summary.aliases_applied);
        }
        for (name, value) in &config.aliases {
            if !apply_alias(&mut executor, name, value) {
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
    }

    fn run_native_chpwd_plugins(&mut self) {
        if self.native_plugin_enabled("direnv") {
            self.apply_direnv_export();
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
        let output = match Command::new("direnv")
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

    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
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
}
