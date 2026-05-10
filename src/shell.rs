use colored::Colorize;
use log::debug;
use reedline::{
    default_emacs_keybindings, DefaultPrompt, DefaultPromptSegment, Emacs,
    DescriptionPosition, FileBackedHistory, KeyCode, KeyModifiers, ListMenu, MenuBuilder,
    Reedline, ReedlineEvent, ReedlineMenu,
};
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::array::ArrayValue;
use crate::config::ShellConfig;
use crate::error::{Result, ShellError};
use crate::executor::Executor;
use crate::job::JobManager;
use crate::parser::Parser;
use crate::plugin::PluginManager;
use crate::theme::ThemePlugin;
use crate::tokenizer::{CommandInfo, ParsedCommand, Tokenizer};
use glob;

/// Main shell structure
pub struct Shell {
    pub current_dir: PathBuf,
    pub aliases: HashMap<String, String>,
    pub env_vars: HashMap<String, ArrayValue>,
    pub line_editor: Reedline,
    pub history_path: PathBuf,
    pub config: ShellConfig,
    pub plugins: PluginManager,
    pub job_manager: JobManager,
    pub theme_plugin: ThemePlugin,
    pub last_exit_code: i32,
    pub command_router: Option<crate::command_router::CommandRouter>,
    // Store completion state reference for updates
    completion_state: std::sync::Arc<std::sync::Mutex<crate::completion::CompletionState>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptFlow {
    None,
    Break,
    Continue,
}

#[derive(Debug, Default)]
struct ScriptState {
    positional: Vec<String>,
    locals: HashMap<String, String>,
    functions: HashMap<String, Vec<String>>,
}

impl ScriptState {
    fn new(args: &[String]) -> Self {
        Self {
            positional: args.to_vec(),
            locals: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    fn shift(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        if n >= self.positional.len() {
            self.positional.clear();
            return;
        }
        self.positional.drain(0..n);
    }

    fn positional(&self, index: usize) -> Option<&str> {
        self.positional.get(index).map(|s| s.as_str())
    }
}

impl Shell {
    /// Create a new shell instance
    pub fn new(load_config: bool) -> Result<Self> {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let history_path = home_dir.join(".winsh_history");

        // Create shared completion state with current directory
        let current_dir = std::env::current_dir()?;
        let completion_state = std::sync::Arc::new(std::sync::Mutex::new(
            crate::completion::CompletionState::new(current_dir.clone())
        ));

        // Register built-in command completion plugin
        {
            use crate::completion::external::CommandCompletionPlugin;
            if let Ok(mut state) = completion_state.lock() {
                state.add_plugin(std::sync::Arc::new(CommandCompletionPlugin));
            }
        }

        // Create custom completer with shared state
        use crate::completion::WinuxshCompleter;
        let completer = Box::new(WinuxshCompleter::new(
            completion_state.clone()
        ));

        // Create completion menu — single-column popup style
        use nu_ansi_term::{Color, Style};
        let completion_menu = Box::new(
            ListMenu::default()
                .with_name("completion_menu")
                // Pass the full line buffer to the completer (not just the diff)
                .with_only_buffer_difference(false)
                // prefix marker for the selected row
                .with_marker("> ")
                // normal (unselected) item
                .with_text_style(
                    Style::new().fg(Color::White)
                )
                // selected item: bright cyan bg, black fg
                .with_selected_text_style(
                    Style::new().fg(Color::Black).on(Color::Fixed(39))
                )
                // matched chars (typed prefix) shown bold green
                .with_match_text_style(
                    Style::new().fg(Color::Fixed(114)).bold()
                )
                // matched chars inside the selected item
                .with_selected_match_text_style(
                    Style::new().fg(Color::Black).on(Color::Fixed(39)).bold()
                )
                // render description after the completion value
                .with_description_position(DescriptionPosition::After)
                .with_page_size(12),
        );

        // Setup TAB key binding for completion (exactly like MVP4)
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Tab,
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::Menu("completion_menu".to_string()),
                ReedlineEvent::MenuNext,
            ]),
        );

        let edit_mode = Box::new(Emacs::new(keybindings));

        // Create history with a graceful fallback when file access is denied.
        // This keeps tests and restricted environments from panicking.
        let history = match FileBackedHistory::with_file(1000, history_path.clone()) {
            Ok(h) => h,
            Err(e) => {
                eprintln!(
                    "{} {}",
                    "Warning:".yellow(),
                    format!(
                        "Failed to open history file, using in-memory history: {}",
                        e
                    )
                );
                match FileBackedHistory::new(1000) {
                    Ok(h) => h,
                    Err(fallback_err) => {
                        return Err(crate::error::ShellError::Config(format!(
                            "Failed to initialize history: {}; fallback failed: {}",
                            e, fallback_err
                        )));
                    }
                }
            }
        };

        // Create line editor with edit mode and completion (exactly like MVP4)
        let line_editor = Reedline::create()
            .with_completer(completer)
            .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
            .with_edit_mode(edit_mode)
            .with_history(Box::new(history))
            .with_quick_completions(false)
            .with_partial_completions(true);

        // Load command classification and create router
        let command_router = match crate::command_router::load_classification() {
            Ok(classification) => {
                let router = crate::command_router::CommandRouter::new(classification, true); // Default to enabled, will be updated by config
                Some(router)
            }
            Err(e) => {
                eprintln!(
                    "{} {}",
                    "Warning:".yellow(),
                    format!("Failed to load command classification: {}", e)
                );
                None
            }
        };

        let mut shell = Shell {
            current_dir: std::env::current_dir()?,
            aliases: HashMap::new(),
            env_vars: HashMap::new(),
            line_editor,
            history_path,
            config: ShellConfig::default(),
            plugins: PluginManager::new(),
            job_manager: JobManager::new(),
            theme_plugin: ThemePlugin::new(),
            last_exit_code: 0,
            command_router,
            completion_state,
        };

        // Load default aliases
        shell.aliases.insert("ll".to_string(), "ls -la".to_string());
        shell.aliases.insert("la".to_string(), "ls -a".to_string());
        shell.aliases.insert("l".to_string(), "ls".to_string());

        // Load environment variables
        for (key, value) in std::env::vars() {
            shell.env_vars.insert(key, ArrayValue::String(value));
        }

        // Automatically add winuxcmd directory to PATH (MVP5 compatibility)
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let winuxcmd_dir = exe_dir.join("winuxcmd");
                if winuxcmd_dir.exists() {
                    let winuxcmd_path = winuxcmd_dir.to_string_lossy().to_string();
                    if let Some(path_value) = shell
                        .env_vars
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
                        .map(|(_, v)| v.clone())
                    {
                        // Add winuxcmd to end of PATH to avoid affecting system commands
                        let new_path = format!("{};{}", path_value, winuxcmd_path);
                        shell
                            .env_vars
                            .insert("PATH".to_string(), ArrayValue::String(new_path.clone()));
                        std::env::set_var("PATH", &new_path);
                    } else {
                        shell.env_vars.insert(
                            "PATH".to_string(),
                            ArrayValue::String(winuxcmd_path.clone()),
                        );
                        std::env::set_var("PATH", &winuxcmd_path);
                    }
                }
            }
        }

        // Load configuration
        if load_config {
            if let Err(e) = shell.load_config() {
                eprintln!(
                    "{} {}",
                    "Warning:".yellow(),
                    format!("Failed to load config: {}", e)
                );
            } else {
                // Update command_router with config settings
                if let Some(router) = &mut shell.command_router {
                    let enable_dll = shell.config.winuxcmd.enable_dll;
                    router.set_enable_dll(enable_dll);
                }
            }
            // Always register external completion plugin (uses defaults when config failed)
            shell.register_external_completion_plugin();
        }

        // Initialize plugins
        use crate::oh_my_winuxsh::OhMyWinuxsh;
        use crate::plugin::WelcomePlugin;

        // Add welcome plugin
        if let Err(e) = shell.plugins.add_plugin(Box::new(WelcomePlugin)) {
            eprintln!(
                "{} {}",
                "Warning:".yellow(),
                format!("Failed to load welcome plugin: {}", e)
            );
        }

        // Add oh-my-winuxsh plugin
        if let Err(e) = shell.plugins.add_plugin(Box::new(OhMyWinuxsh)) {
            eprintln!(
                "{} {}",
                "Warning:".yellow(),
                format!("Failed to load oh-my-winuxsh plugin: {}", e)
            );
        }

        Ok(shell)
    }

    /// Load configuration
    fn load_config(&mut self) -> Result<()> {
        use crate::config::ConfigManager;

        // Load shell config
        if let Some(config_path) = ConfigManager::find_config_file() {
            if config_path
                .extension()
                .map(|e| e == "toml")
                .unwrap_or(false)
            {
                let mut config_manager = ConfigManager::new();
                self.config = config_manager.load_config(&config_path)?;
            } else {
                self.parse_config_file(&config_path)?;
            }
        }

        Ok(())
    }

    /// Load external completion definitions from the configured directory and
    /// register an `ExternalCompletionPlugin` into the completion state.
    fn register_external_completion_plugin(&self) {
        use crate::completion::external::ExternalCompletionPlugin;

        // Collect directories: configured ones + default ~/.winsh/completions
        let mut dirs: Vec<PathBuf> = self.config.completions.completion_dirs.iter().map(|dir| {
            if dir.starts_with('~') {
                if let Some(home) = dirs::home_dir() {
                    home.join(&dir[2..]) // skip "~/"
                } else {
                    PathBuf::from(dir)
                }
            } else {
                PathBuf::from(dir)
            }
        }).collect();

        // Always include the default dir
        if let Some(home) = dirs::home_dir() {
            let default_dir = home.join(".winsh").join("completions");
            if !dirs.contains(&default_dir) {
                dirs.push(default_dir);
            }
        }

        let mut plugin = ExternalCompletionPlugin::new();

        for dir in &dirs {
            if !dir.exists() {
                log::debug!("External completion dir {:?} does not exist, skipping", dir);
                continue;
            }
            plugin.load_dir(dir);
        }

        // Enrich flag descriptions from `cmd -h` output
        plugin.enrich_descriptions_from_help();

        if plugin.definition_count() > 0 {
            if let Ok(mut state) = self.completion_state.lock() {
                state.add_plugin(std::sync::Arc::new(plugin));
            }
        }
    }

    /// Parse configuration file
    pub fn parse_config_file(&mut self, path: &Path) -> Result<()> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Execute config commands
            // TODO: Implement proper command execution
            let _ = line;
        }

        Ok(())
    }

    /// Update completion state
    pub fn update_completion_state(&self) {
        if let Ok(mut state) = self.completion_state.lock() {
            state.current_dir = self.current_dir.clone();
            state.env_vars = self.env_vars.clone();
        }
    }

    /// Get environment variable
    pub fn get_env_var(&self, key: &str, default: &str) -> String {
        if let Some(value) = self.env_vars.get(key) {
            return value.as_string().unwrap_or(default).to_string();
        }

        self.env_vars
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .and_then(|(_, v)| v.as_string().map(|s| s.to_string()))
            .unwrap_or_else(|| default.to_string())
    }

    /// Get the prompt string
    pub fn get_prompt(&self) -> DefaultPrompt {
        let username = self.get_env_var("USERNAME", "user");
        let hostname = self.get_env_var("COMPUTERNAME", "localhost");
        let dir = self.current_dir.display().to_string();

        // Use theme plugin if available, otherwise use default colors
        let prompt_text = if let ThemePlugin::Theme(ref theme) = self.theme_plugin {
            theme.generate_prompt(&username, &hostname, &dir, "$ ")
        } else {
            // Default colored prompt
            format!(
                "\x1b[1;32m{}@{}\x1b[0m \x1b[1;34m{}\x1b[0m $ ",
                username, hostname, dir
            )
        };

        DefaultPrompt::new(
            DefaultPromptSegment::Basic(prompt_text),
            DefaultPromptSegment::Empty,
        )
    }

    /// Parse ANSI escape sequences from config format
    fn parse_ansi_sequence(&self, input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(&c) = chars.peek() {
            chars.next();

            if c == '\\' {
                if let Some(&'x') = chars.peek() {
                    chars.next(); // consume 'x'
                    if let Some(&'1') = chars.peek() {
                        chars.next(); // consume '1'
                        if let Some(&'b') = chars.peek() {
                            chars.next(); // consume 'b'
                                          // This is \x1b, add actual ANSI escape
                            result.push('\x1b');
                        } else {
                            result.push_str("\\x1");
                        }
                    } else {
                        result.push_str("\\x");
                    }
                } else {
                    result.push(c);
                }
            } else if c == '\x1b' {
                // Already an escape sequence, keep it
                result.push(c);
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Save command to history
    pub fn save_history(&mut self, command: &str) -> Result<()> {
        let clean_command =
            command.trim_matches(|c: char| c == '\u{feff}' || c == '\u{fffe}' || c.is_whitespace());

        if clean_command.is_empty() {
            return Ok(());
        }

        let mut history = if self.history_path.exists() {
            std::fs::read_to_string(&self.history_path)?
        } else {
            String::new()
        };

        if !history.is_empty() {
            history.push('\n');
        }
        history.push_str(clean_command);

        std::fs::write(&self.history_path, history)?;

        Ok(())
    }

    /// Execute a command
    pub fn execute_command(&mut self, command: &str) -> Result<()> {
        // Tokenize the command
        let tokens = Tokenizer::tokenize(command)?;

        // Parse the tokens into an AST
        let parsed = Parser::parse(&tokens)?;

        // Execute the parsed command
        self.execute_parsed(&parsed)?;

        Ok(())
    }

    /// Execute a parsed command
    pub fn execute_parsed(&mut self, parsed: &ParsedCommand) -> Result<()> {
        match parsed {
            ParsedCommand::Single(cmd) => {
                self.execute_single_command(cmd)?;
            }
            ParsedCommand::Pipeline(cmds) => {
                self.execute_pipeline(cmds)?;
            }
            ParsedCommand::And(left, right) => {
                // Execute left command, only execute right if exit code is 0
                self.execute_parsed(left)?;
                if self.last_exit_code == 0 {
                    self.execute_parsed(right)?;
                }
            }
            ParsedCommand::Or(left, right) => {
                // Execute left command, only execute right if exit code is non-zero
                self.execute_parsed(left)?;
                if self.last_exit_code != 0 {
                    self.execute_parsed(right)?;
                }
            }
            ParsedCommand::Sequence(cmds) => {
                // Execute commands in sequence
                for cmd in cmds {
                    self.execute_parsed(cmd)?;
                }
            }
        }
        Ok(())
    }

    /// Execute a single command
    pub fn execute_single_command(&mut self, cmd: &CommandInfo) -> Result<()> {
        // Skip empty commands
        if cmd.args.is_empty() {
            return Ok(());
        }

        // Clone the command info for modification
        let mut cmd_clone = cmd.clone();

        // Expand aliases
        let first_arg = &cmd_clone.args[0];
        if let Some(alias_cmd) = self.aliases.get(first_arg) {
            let alias_parts: Vec<String> = alias_cmd
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            if !alias_parts.is_empty() {
                cmd_clone.args[0] = alias_parts[0].clone();
                cmd_clone
                    .args
                    .splice(1..1, alias_parts[1..].iter().cloned());
            }
        }

        // Get command name
        let clean_command = cmd_clone.args[0]
            .trim_matches(|c: char| c == '\u{feff}' || c == '\u{fffe}' || c.is_whitespace())
            .to_string();

        // Expand command substitution in arguments
        let args_with_substitution: Vec<String> = cmd_clone.args[1..]
            .iter()
            .map(|arg| self.expand_command_substitution(arg))
            .collect();

        // Expand wildcards in arguments (skip the command name)
        let expanded_args = self.expand_wildcards(&args_with_substitution);

        // Combine command name with expanded arguments
        let all_args: Vec<String> = vec![clean_command.clone()]
            .into_iter()
            .chain(expanded_args)
            .collect();

        // Always prefer builtin handling first.
        // This avoids router/classification drift causing builtins (e.g. source)
        // to be misrouted as external commands.
        if self.try_builtin_with_redirection(&clean_command, &all_args, &cmd_clone)? {
            self.last_exit_code = 0;
            return Ok(());
        }

        if let Some(result) = self.handle_builtin(&all_args) {
            match result {
                Ok(()) => {
                    self.last_exit_code = 0;
                    return Ok(());
                }
                Err(e) => {
                    self.last_exit_code = 1;
                    if clean_command != "[" && clean_command != "test" {
                        eprintln!("{} {}", "Error:".red(), e);
                    }
                    return Ok(());
                }
            }
        }

        // Route command using command_router if available
        if let Some(router) = &self.command_router {
            let route_decision = router.route_command(&clean_command);

            match route_decision {
                crate::command_router::RouteDecision::Builtin => {
                    // Builtins are already handled above; keep this as a no-op.
                }
                crate::command_router::RouteDecision::WinuxCmdDLL(category) => {
                    // Execute via WinuxCmd DLL
                    let args: Vec<String> = all_args[1..].to_vec();
                    return self.execute_winuxcmd_command(&clean_command, &args, &cmd_clone);
                }
                crate::command_router::RouteDecision::ExternalCommand => {
                    // Execute via PATH (fall through)
                }
                crate::command_router::RouteDecision::NotFound => {
                    self.last_exit_code = 127;
                    eprintln!("{} {}", "Error:".red(), format!("Command '{}' not found", clean_command));
                    return Ok(());
                }
            }
        }

        // Execute external command via PATH
        let args: Vec<String> = all_args[1..].to_vec();

        // Convert environment variables to the format expected by Executor
        let env_vars: Vec<(String, ArrayValue)> = self
            .env_vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Create executor
        let executor = Executor::new(&env_vars, &self.current_dir);

        // Execute the external command
        let mut cmd_info = cmd_clone;
        cmd_info.args = all_args;

        match executor.execute(&clean_command, &args, &cmd_info) {
            Ok(exit_code) => {
                self.last_exit_code = exit_code;
                Ok(())
            }
            Err(e) => {
                self.last_exit_code = match e {
                    ShellError::CommandNotFound(_) => 127,
                    _ => 1,
                };
                eprintln!("{} {}", "Error:".red(), e);
                Ok(())
            }
        }
    }

    /// Execute WinuxCmd command via DLL
    fn execute_winuxcmd_command(
        &mut self,
        command: &str,
        args: &[String],
        cmd_info: &CommandInfo,
    ) -> Result<()> {
        use crate::winuxcmd_ffi::WinuxCmdFFI;

        debug!("Executing via WinuxCmd DLL: {} {:?}", command, args);

        // Check for stdin redirection - DLL may not support this
        if cmd_info.stdin_redir.is_some() {
            log::debug!("Command has stdin redirection, falling back to external execution");
            return self.execute_external_command_fallback(command, args, cmd_info);
        }

        match WinuxCmdFFI::execute(command, args) {
            Ok(response) => {
                // Handle stdout redirection
                if let Some(ref stdout_file) = cmd_info.stdout_redir {
                    // Redirect stdout to file
                    use std::io::Write;
                    let mut file = if cmd_info.stdout_append {
                        std::fs::OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open(stdout_file)?
                    } else {
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(true)
                            .open(stdout_file)?
                    };
                    if !response.stdout.is_empty() {
                        file.write_all(&response.stdout)?;
                    }
                } else {
                    // Print output as raw bytes to preserve ANSI codes
                    if !response.stdout.is_empty() {
                        let stdout_str = String::from_utf8_lossy(&response.stdout);
                        print!("{}", stdout_str);
                    }
                }

                // Handle stderr redirection
                if let Some(ref stderr_file) = cmd_info.stderr_redir {
                    // Redirect stderr to file
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(stderr_file)?;
                    if !response.stderr.is_empty() {
                        file.write_all(&response.stderr)?;
                    }
                } else if cmd_info.stderr_to_stdout {
                    // Redirect stderr to stdout
                    if let Some(ref stdout_file) = cmd_info.stdout_redir {
                        use std::io::Write;
                        let mut file = if cmd_info.stdout_append {
                            std::fs::OpenOptions::new()
                                .append(true)
                                .create(true)
                                .open(stdout_file)?
                        } else {
                            std::fs::OpenOptions::new()
                                .write(true)
                                .create(true)
                                .truncate(true)
                                .open(stdout_file)?
                        };
                        if !response.stderr.is_empty() {
                            file.write_all(&response.stderr)?;
                        }
                    } else {
                        // Print to stdout
                        if !response.stderr.is_empty() {
                            let stderr_str = String::from_utf8_lossy(&response.stderr);
                            print!("{}", stderr_str);
                        }
                    }
                } else {
                    // Print stderr to terminal
                    if !response.stderr.is_empty() {
                        let stderr_str = String::from_utf8_lossy(&response.stderr);
                        eprint!("{}", stderr_str);
                    }
                }

                self.last_exit_code = response.exit_code;

                if response.exit_code != 0 {
                    eprintln!("Command exited with status code: {}", response.exit_code);
                }

                Ok(())
            }
            Err(e) => {
                // DLL execution failed, fall back to external command
                eprintln!("{} {}", "Warning:".yellow(), format!("WinuxCmd DLL failed: {}", e));
                eprintln!("Falling back to external command execution");
                self.execute_external_command_fallback(command, args, cmd_info)
            }
        }
    }

    /// Execute command via external PATH as fallback
    fn execute_external_command_fallback(
        &mut self,
        command: &str,
        args: &[String],
        cmd_info: &CommandInfo,
    ) -> Result<()> {
        debug!("Executing via PATH: {} {:?}", command, args);

        // Convert environment variables to the format expected by Executor
        let env_vars: Vec<(String, ArrayValue)> = self
            .env_vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Create executor
        let executor = Executor::new(&env_vars, &self.current_dir);

        // Create modified command info with proper args
        let mut cmd_info_clone = cmd_info.clone();
        cmd_info_clone.args = vec![command.to_string()]
            .into_iter()
            .chain(args.iter().cloned())
            .collect();

        // Execute the external command
        let actual_args: Vec<String> = args.to_vec();
        match executor.execute(command, &actual_args, &cmd_info_clone) {
            Ok(exit_code) => {
                self.last_exit_code = exit_code;
                Ok(())
            }
            Err(e) => {
                self.last_exit_code = match e {
                    ShellError::CommandNotFound(_) => 127,
                    _ => 1,
                };
                eprintln!("{} {}", "Error:".red(), e);
                Ok(())
            }
        }
    }

    /// Execute a pipeline
    pub fn execute_pipeline(&mut self, cmds: &[CommandInfo]) -> Result<()> {
        log::debug!("execute_pipeline: {} commands", cmds.len());
        
        if cmds.is_empty() {
            return Ok(());
        }

        if cmds.len() == 1 {
            // Single command, no pipeline needed
            return self.execute_single_command(&cmds[0]);
        }

        // Fast path: support selected builtins as the first stage in a pipeline.
        // Example: env | grep PATH
        if let Some(input) = self.try_builtin_pipeline_input(&cmds[0]) {
            return self.execute_real_pipeline_with_input(&cmds[1..], input.into_bytes());
        }

        // Use real pipeline implementation with Windows pipes
        self.execute_real_pipeline(cmds)
    }

    fn try_builtin_pipeline_input(&self, first: &CommandInfo) -> Option<String> {
        if first.args.is_empty() {
            return None;
        }
        let cmd = first.args[0].as_str();
        match cmd {
            "env" => {
                let mut output = String::new();
                for (key, value) in &self.env_vars {
                    match value {
                        ArrayValue::String(v) => {
                            output.push_str(&format!("{}={}\n", key, v));
                        }
                        ArrayValue::Array(arr) => {
                            output.push_str(&format!("{}=({})\n", key, arr.join(" ")));
                        }
                    }
                }
                Some(output)
            }
            "echo" => {
                let text = first.args[1..].join(" ");
                Some(format!("{text}\n"))
            }
            "pwd" => Some(format!("{}\n", self.current_dir.display())),
            _ => None,
        }
    }

    /// Execute a real pipeline with Windows pipes
    fn execute_real_pipeline(&mut self, cmds: &[CommandInfo]) -> Result<()> {
        use std::process::{Child, Stdio, Command};

        if cmds.is_empty() {
            return Ok(());
        }

        // Convert environment variables
        let env_vars: Vec<(String, String)> = self
            .env_vars
            .iter()
            .filter_map(|(k, v)| {
                if let ArrayValue::String(ref s) = v {
                    Some((k.clone(), s.clone()))
                } else {
                    None
                }
            })
            .collect();

        let mut children: Vec<Child> = Vec::new();
        let mut prev_stdout: Option<std::process::ChildStdout> = None;

        for (i, cmd) in cmds.iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == cmds.len() - 1;

            // Get command name and args
            if cmd.args.is_empty() {
                return Err(crate::error::ShellError::Parse("Empty command in pipeline".to_string()));
            }

            let cmd_name = &cmd.args[0];
            let cmd_args = &cmd.args[1..];

            let route_decision = if let Some(router) = &self.command_router {
                router.route_command(cmd_name)
            } else {
                crate::command_router::RouteDecision::ExternalCommand
            };

            // Check if this is a builtin command - handle specially
            let is_builtin = matches!(route_decision, crate::command_router::RouteDecision::Builtin);

            // For now, allow builtin commands to pass through (they will fail, but won't block other commands)
            // TODO: Implement proper builtin command pipeline support
            if is_builtin {
                // For builtin commands in pipelines, just continue - they will fail to find the executable
            }

            // Resolve executable and argv for this pipeline stage.
            let (program, stage_args): (String, Vec<String>) = match route_decision {
                crate::command_router::RouteDecision::WinuxCmdDLL(_) => {
                    let winuxcmd_bin = self
                        .find_command_in_path("winuxcmd")
                        .or_else(|| self.find_winuxcmd_binary_path())
                        .ok_or_else(|| crate::error::ShellError::CommandNotFound(
                            "winuxcmd executable not found for pipeline stage".to_string(),
                        ))?;
                    let mut args = Vec::with_capacity(cmd_args.len() + 1);
                    args.push(cmd_name.to_string());
                    args.extend(cmd_args.iter().cloned());
                    (winuxcmd_bin.to_string_lossy().to_string(), args)
                }
                _ => {
                    if let Some(cmd_path) = self.find_command_in_path(cmd_name) {
                        (
                            cmd_path.to_string_lossy().to_string(),
                            cmd_args.to_vec(),
                        )
                    } else if self.is_winuxcmd_classified(cmd_name) {
                        let winuxcmd_bin = self
                            .find_command_in_path("winuxcmd")
                            .or_else(|| self.find_winuxcmd_binary_path())
                            .ok_or_else(|| crate::error::ShellError::CommandNotFound(
                                format!("Command '{}' not found", cmd_name),
                            ))?;
                        let mut args = Vec::with_capacity(cmd_args.len() + 1);
                        args.push(cmd_name.to_string());
                        args.extend(cmd_args.iter().cloned());
                        (winuxcmd_bin.to_string_lossy().to_string(), args)
                    } else {
                        return Err(crate::error::ShellError::CommandNotFound(format!(
                            "Command '{}' not found",
                            cmd_name
                        )));
                    }
                }
            };

            // Create process
            let mut process = Command::new(&program);
            process.args(&stage_args);
            process.envs(env_vars.iter().cloned());
            process.current_dir(&self.current_dir);

            // Set up stdin - CRITICAL: use from_stdin for pipe connections
            if let Some(stdout) = prev_stdout.take() {
                process.stdin(Stdio::from(stdout));
            } else if let Some(ref stdin_file) = cmd.stdin_redir {
                let file = std::fs::File::open(stdin_file)?;
                process.stdin(Stdio::from(file));
            } else {
                process.stdin(Stdio::inherit());
            }

            // Set up stdout - CRITICAL: must be piped for non-last commands
            if is_last {
                // Last command: stdout goes to terminal or file
                if let Some(ref stdout_file) = cmd.stdout_redir {
                    let file = if cmd.stdout_append {
                        std::fs::OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open(stdout_file)?
                    } else {
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(true)
                            .open(stdout_file)?
                    };
                    process.stdout(Stdio::from(file));
                } else {
                    process.stdout(Stdio::inherit());
                }
            } else {
                // Not last command: stdout MUST be piped for pipe connections
                process.stdout(Stdio::piped());
            }

            // Set up stderr
            if let Some(ref stderr_file) = cmd.stderr_redir {
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(stderr_file)?;
                process.stderr(Stdio::from(file));
            } else if cmd.stderr_to_stdout {
                process.stderr(Stdio::inherit());
            } else {
                process.stderr(Stdio::inherit());
            }

            // Create new process group on Windows to prevent Ctrl+C from killing the shell
            // CRITICAL: This must be applied to ALL child processes
            #[cfg(windows)]
            {
                const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
                process.creation_flags(CREATE_NEW_PROCESS_GROUP);
            }

            // Spawn process
            let mut child = process.spawn()
                .map_err(|e| crate::error::ShellError::CommandNotFound(format!(
                    "Failed to execute '{}': {}",
                    cmd_name, e
                )))?;

            // Debug output
            log::debug!("Spawned process '{}' (PID: {:?}) in pipeline (is_last: {})", 
                       cmd_name, child.id(), is_last);

            // Save stdout for next command (if not last)
            if !is_last {
                prev_stdout = child.stdout.take();
            }

            children.push(child);
        }

        // Close the previous stdout to signal EOF to the next process
        drop(prev_stdout);

        // Wait for all processes to complete
        let mut last_exit_code = 0;
        for mut child in children {
            let status = child.wait()
                .map_err(|e| crate::error::ShellError::CommandNotFound(format!(
                    "Failed to wait for process: {}", e
                )))?;

            last_exit_code = status.code().unwrap_or(1);
        }

        self.last_exit_code = last_exit_code;
        Ok(())
    }

    fn execute_real_pipeline_with_input(&mut self, cmds: &[CommandInfo], input: Vec<u8>) -> Result<()> {
        use std::io::Write;
        use std::process::{Child, Command, Stdio};

        if cmds.is_empty() {
            self.last_exit_code = 0;
            return Ok(());
        }

        let env_vars: Vec<(String, String)> = self
            .env_vars
            .iter()
            .filter_map(|(k, v)| {
                if let ArrayValue::String(ref s) = v {
                    Some((k.clone(), s.clone()))
                } else {
                    None
                }
            })
            .collect();

        let mut children: Vec<Child> = Vec::new();
        let mut prev_stdout: Option<std::process::ChildStdout> = None;

        for (i, cmd) in cmds.iter().enumerate() {
            let is_last = i == cmds.len() - 1;
            if cmd.args.is_empty() {
                return Err(crate::error::ShellError::Parse("Empty command in pipeline".to_string()));
            }

            let cmd_name = &cmd.args[0];
            let cmd_args = &cmd.args[1..];
            let route_decision = if let Some(router) = &self.command_router {
                router.route_command(cmd_name)
            } else {
                crate::command_router::RouteDecision::ExternalCommand
            };

            let (program, stage_args): (String, Vec<String>) = match route_decision {
                crate::command_router::RouteDecision::WinuxCmdDLL(_) => {
                    let winuxcmd_bin = self
                        .find_command_in_path("winuxcmd")
                        .or_else(|| self.find_winuxcmd_binary_path())
                        .ok_or_else(|| crate::error::ShellError::CommandNotFound(
                            "winuxcmd executable not found for pipeline stage".to_string(),
                        ))?;
                    let mut args = Vec::with_capacity(cmd_args.len() + 1);
                    args.push(cmd_name.to_string());
                    args.extend(cmd_args.iter().cloned());
                    (winuxcmd_bin.to_string_lossy().to_string(), args)
                }
                _ => {
                    if let Some(cmd_path) = self.find_command_in_path(cmd_name) {
                        (
                            cmd_path.to_string_lossy().to_string(),
                            cmd_args.to_vec(),
                        )
                    } else if self.is_winuxcmd_classified(cmd_name) {
                        let winuxcmd_bin = self
                            .find_command_in_path("winuxcmd")
                            .or_else(|| self.find_winuxcmd_binary_path())
                            .ok_or_else(|| crate::error::ShellError::CommandNotFound(
                                format!("Command '{}' not found", cmd_name),
                            ))?;
                        let mut args = Vec::with_capacity(cmd_args.len() + 1);
                        args.push(cmd_name.to_string());
                        args.extend(cmd_args.iter().cloned());
                        (winuxcmd_bin.to_string_lossy().to_string(), args)
                    } else {
                        return Err(crate::error::ShellError::CommandNotFound(format!(
                            "Command '{}' not found",
                            cmd_name
                        )));
                    }
                }
            };

            let mut process = Command::new(&program);
            process.args(&stage_args);
            process.envs(env_vars.iter().cloned());
            process.current_dir(&self.current_dir);

            if i == 0 {
                process.stdin(Stdio::piped());
            } else if let Some(stdout) = prev_stdout.take() {
                process.stdin(Stdio::from(stdout));
            } else {
                process.stdin(Stdio::inherit());
            }

            if is_last {
                process.stdout(Stdio::inherit());
            } else {
                process.stdout(Stdio::piped());
            }
            process.stderr(Stdio::inherit());

            #[cfg(windows)]
            {
                const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
                process.creation_flags(CREATE_NEW_PROCESS_GROUP);
            }

            let mut child = process.spawn().map_err(|e| {
                crate::error::ShellError::CommandNotFound(format!(
                    "Failed to execute '{}': {}",
                    cmd_name, e
                ))
            })?;

            if i == 0 {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(&input);
                }
            }

            if !is_last {
                prev_stdout = child.stdout.take();
            }
            children.push(child);
        }

        let mut last_exit_code = 0;
        for mut child in children {
            let status = child.wait().map_err(|e| {
                crate::error::ShellError::CommandNotFound(format!(
                    "Failed to wait for process: {}",
                    e
                ))
            })?;
            last_exit_code = status.code().unwrap_or(1);
        }
        self.last_exit_code = last_exit_code;
        Ok(())
    }

    fn find_winuxcmd_binary_path(&self) -> Option<PathBuf> {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let candidate = exe_dir.join("winuxcmd").join("winuxcmd.exe");
                if candidate.exists() {
                    return Some(candidate);
                }
                // Typical dev layout: target/debug/winuxsh.exe -> ../../utils/winuxcmd/winuxcmd.exe
                if let Some(target_dir) = exe_dir.parent() {
                    if let Some(repo_root) = target_dir.parent() {
                        let candidate2 =
                            repo_root.join("utils").join("winuxcmd").join("winuxcmd.exe");
                        if candidate2.exists() {
                            return Some(candidate2);
                        }
                    }
                }
            }
        }

        let repo_candidate = self.current_dir.join("utils").join("winuxcmd").join("winuxcmd.exe");
        if repo_candidate.exists() {
            return Some(repo_candidate);
        }

        None
    }

    fn is_winuxcmd_classified(&self, cmd_name: &str) -> bool {
        if let Some(router) = &self.command_router {
            router.classification().is_winuxcmd_command(cmd_name)
        } else {
            false
        }
    }

    /// Find command in PATH (helper method for pipeline)
    fn find_command_in_path(&self, cmd: &str) -> Option<PathBuf> {
        let env_vars: Vec<(String, ArrayValue)> = self
            .env_vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let executor = Executor::new(&env_vars, &self.current_dir);
        match executor.find_command_in_path(cmd) {
            Ok(path) => path,
            Err(_) => None,
        }
    }

    /// Expand wildcards in arguments
    pub fn expand_wildcards(&self, args: &[String]) -> Vec<String> {
        let mut expanded = Vec::new();

        for arg in args {
            if arg.contains('*') || arg.contains('?') || arg.contains('[') {
                // Expand wildcard per argument so fallback decisions do not depend on prior args.
                if let Ok(matches) = glob::glob(arg) {
                    let mut matched_paths = Vec::new();
                    for entry in matches.flatten() {
                        matched_paths.push(entry.to_string_lossy().to_string());
                    }

                    if matched_paths.is_empty() {
                        expanded.push(arg.clone());
                    } else {
                        expanded.extend(matched_paths);
                    }
                } else {
                    // Invalid pattern, keep original
                    expanded.push(arg.clone());
                }
            } else {
                // No wildcard, keep as is
                expanded.push(arg.clone());
            }
        }

        expanded
    }

    /// Expand command substitution $(...)
    pub fn expand_command_substitution(&mut self, input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '$' {
                if let Some(&'(') = chars.peek() {
                    // Start of command substitution
                    chars.next(); // consume '('
                    let mut command = String::new();
                    let mut depth = 1;

                    while let Some(&c) = chars.peek() {
                        chars.next(); // consume char
                        if c == '(' {
                            depth += 1;
                            command.push(c);
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break; // End of command substitution
                            } else {
                                command.push(c);
                            }
                        } else {
                            command.push(c);
                        }
                    }

                    // Execute the command and capture output
                    let output = self.execute_substitution_command(&command);
                    result.push_str(&output.trim());
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    fn execute_substitution_command(&mut self, command: &str) -> String {
        let tokens = match Tokenizer::tokenize(command) {
            Ok(tokens) => tokens,
            Err(_) => return String::new(),
        };

        let parsed = match Parser::parse(&tokens) {
            Ok(parsed) => parsed,
            Err(_) => return String::new(),
        };

        let (stdout_path, stderr_path) = self.create_substitution_capture_paths();
        let redirected = self.redirect_parsed_for_capture(&parsed, &stdout_path, &stderr_path);
        let _ = self.execute_parsed(&redirected);

        let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
        let _ = std::fs::remove_file(&stdout_path);
        let _ = std::fs::remove_file(&stderr_path);
        stdout
    }

    fn create_substitution_capture_paths(&self) -> (PathBuf, PathBuf) {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "winuxsh_subst_{}_{}",
            std::process::id(),
            nonce
        ));

        let stdout_path = base.with_extension("stdout.tmp");
        let stderr_path = base.with_extension("stderr.tmp");
        (stdout_path, stderr_path)
    }

    fn redirect_parsed_for_capture(
        &self,
        parsed: &ParsedCommand,
        stdout_path: &Path,
        stderr_path: &Path,
    ) -> ParsedCommand {
        match parsed {
            ParsedCommand::Single(cmd) => ParsedCommand::Single(
                self.redirect_command_info_for_capture(cmd, stdout_path, stderr_path, true),
            ),
            ParsedCommand::Pipeline(cmds) => {
                let mut redirected = cmds.clone();
                if let Some((last, prefix)) = redirected.split_last_mut() {
                    for cmd in prefix.iter_mut() {
                        if cmd.stderr_redir.is_none() {
                            cmd.stderr_redir = Some(stderr_path.to_string_lossy().to_string());
                            cmd.stderr_append = true;
                        }
                    }
                    *last = self.redirect_command_info_for_capture(last, stdout_path, stderr_path, true);
                }
                ParsedCommand::Pipeline(redirected)
            }
            ParsedCommand::And(left, right) => ParsedCommand::And(
                Box::new(self.redirect_parsed_for_capture(left, stdout_path, stderr_path)),
                Box::new(self.redirect_parsed_for_capture(right, stdout_path, stderr_path)),
            ),
            ParsedCommand::Or(left, right) => ParsedCommand::Or(
                Box::new(self.redirect_parsed_for_capture(left, stdout_path, stderr_path)),
                Box::new(self.redirect_parsed_for_capture(right, stdout_path, stderr_path)),
            ),
            ParsedCommand::Sequence(commands) => ParsedCommand::Sequence(
                commands
                    .iter()
                    .map(|command| self.redirect_parsed_for_capture(command, stdout_path, stderr_path))
                    .collect(),
            ),
        }
    }

    fn redirect_command_info_for_capture(
        &self,
        cmd: &CommandInfo,
        stdout_path: &Path,
        stderr_path: &Path,
        capture_stdout: bool,
    ) -> CommandInfo {
        let mut redirected = cmd.clone();

        if capture_stdout && redirected.stdout_redir.is_none() {
            redirected.stdout_redir = Some(stdout_path.to_string_lossy().to_string());
            redirected.stdout_append = true;
        }

        if redirected.stderr_redir.is_none() {
            redirected.stderr_redir = Some(stderr_path.to_string_lossy().to_string());
            redirected.stderr_append = true;
        }

        redirected
    }

    fn try_builtin_with_redirection(
        &mut self,
        command: &str,
        args: &[String],
        cmd_info: &CommandInfo,
    ) -> Result<bool> {
        let has_redirection = cmd_info.stdin_redir.is_some()
            || cmd_info.stdout_redir.is_some()
            || cmd_info.stderr_redir.is_some()
            || cmd_info.stderr_to_stdout
            || cmd_info.stdout_to_stderr;
        if !has_redirection {
            return Ok(false);
        }

        match command {
            "echo" => {
                let mut output = args[1..].join(" ");
                output.push('\n');
                self.write_builtin_output(&output, cmd_info)?;
                Ok(true)
            }
            "pwd" => {
                let output = format!("{}\n", self.current_dir.display());
                self.write_builtin_output(&output, cmd_info)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn write_builtin_output(&self, output: &str, cmd_info: &CommandInfo) -> Result<()> {
        // If stdout is redirected to stderr (1>&2), emit to stderr destination.
        if cmd_info.stdout_to_stderr {
            if let Some(ref stderr_file) = cmd_info.stderr_redir {
                self.write_to_file(stderr_file, cmd_info.stderr_append, output)?;
            } else {
                eprint!("{}", output);
            }
        } else if let Some(ref stdout_file) = cmd_info.stdout_redir {
            self.write_to_file(stdout_file, cmd_info.stdout_append, output)?;
        } else {
            print!("{}", output);
        }

        // If stderr is redirected to file for a builtin with no stderr output,
        // still materialize the file (bash-compatible touch/truncate behavior).
        if let Some(ref stderr_file) = cmd_info.stderr_redir {
            if !cmd_info.stdout_to_stderr {
                let _ = self.open_file_for_redirect(stderr_file, cmd_info.stderr_append)?;
            }
        }

        Ok(())
    }

    fn write_to_file(&self, path: &str, append: bool, content: &str) -> Result<()> {
        use std::io::Write;
        let mut file = self.open_file_for_redirect(path, append)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    fn open_file_for_redirect(&self, path: &str, append: bool) -> Result<std::fs::File> {
        let file = if append {
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)?
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?
        };
        Ok(file)
    }

    /// Execute a script file with basic script semantics and positional args.
    pub fn run_script_file(&mut self, script_path: &Path, script_args: &[String]) -> Result<()> {
        let script_content = std::fs::read_to_string(script_path)?;
        let lines: Vec<String> = script_content.lines().map(|s| s.to_string()).collect();
        let mut state = ScriptState::new(script_args);
        let _ = self.execute_script_lines(&lines, 0, lines.len(), &mut state)?;
        Ok(())
    }

    fn execute_script_lines(
        &mut self,
        lines: &[String],
        start: usize,
        end: usize,
        state: &mut ScriptState,
    ) -> Result<ScriptFlow> {
        let mut i = start;
        while i < end {
            let line = Self::normalize_script_line(&lines[i]);
            if line.is_empty() || line.starts_with('#') {
                i += 1;
                continue;
            }

            if line == "break" {
                return Ok(ScriptFlow::Break);
            }
            if line == "continue" {
                return Ok(ScriptFlow::Continue);
            }

            if line.starts_with("while ") {
                let (condition, body_start, body_end, next_index) =
                    self.parse_while_block(lines, i, end)?;
                loop {
                    let expanded_condition = self.expand_script_vars(&condition, state);
                    self.execute_command(&expanded_condition)?;
                    if self.last_exit_code != 0 {
                        break;
                    }

                    match self.execute_script_lines(lines, body_start, body_end, state)? {
                        ScriptFlow::None => {}
                        ScriptFlow::Break => break,
                        ScriptFlow::Continue => continue,
                    }
                }
                i = next_index;
                continue;
            }

            if line.starts_with("if ") {
                i = self.execute_if_block(lines, i, end, state)?;
                continue;
            }

            if line.starts_with("case ") {
                i = self.execute_case_block(lines, i, end, state)?;
                continue;
            }

            if let Some(func_name) = Self::parse_function_header(&line) {
                i = self.register_script_function(lines, i, end, func_name, state)?;
                continue;
            }

            self.execute_script_simple_line(&line, state)?;
            i += 1;
        }

        Ok(ScriptFlow::None)
    }

    fn parse_while_block(
        &self,
        lines: &[String],
        start: usize,
        end: usize,
    ) -> Result<(String, usize, usize, usize)> {
        let header = Self::normalize_script_line(&lines[start]);
        let mut condition = header.trim_start_matches("while").trim().to_string();
        let mut body_start = start + 1;

        if condition.ends_with("; do") {
            condition = condition.trim_end_matches("; do").trim().to_string();
        } else if condition.ends_with(" do") {
            condition = condition.trim_end_matches(" do").trim().to_string();
        } else {
            let mut cursor = start + 1;
            while cursor < end {
                let candidate = Self::normalize_script_line(&lines[cursor]);
                if candidate.is_empty() || candidate.starts_with('#') {
                    cursor += 1;
                    continue;
                }
                if candidate == "do" {
                    body_start = cursor + 1;
                    break;
                }
                return Err(ShellError::InvalidCommand(
                    "while syntax expects 'do'".to_string(),
                ));
            }
        }

        if condition.ends_with(';') {
            condition.pop();
            condition = condition.trim().to_string();
        }
        if condition.is_empty() {
            return Err(ShellError::InvalidCommand(
                "while condition cannot be empty".to_string(),
            ));
        }

        let mut depth = 1usize;
        let mut cursor = body_start;
        while cursor < end {
            let candidate = Self::normalize_script_line(&lines[cursor]);
            if candidate.starts_with("while ") {
                depth += 1;
            } else if candidate == "done" {
                depth -= 1;
                if depth == 0 {
                    let body_end = cursor;
                    let next_index = cursor + 1;
                    return Ok((condition, body_start, body_end, next_index));
                }
            }
            cursor += 1;
        }

        Err(ShellError::InvalidCommand(
            "while block missing 'done'".to_string(),
        ))
    }

    fn execute_case_block(
        &mut self,
        lines: &[String],
        start: usize,
        end: usize,
        state: &mut ScriptState,
    ) -> Result<usize> {
        let header = Self::normalize_script_line(&lines[start]);
        if !header.ends_with(" in") {
            return Err(ShellError::InvalidCommand(
                "case syntax expects 'case <word> in'".to_string(),
            ));
        }

        let word_expr = header
            .trim_start_matches("case")
            .trim_end_matches(" in")
            .trim();
        let word_expanded = self.expand_script_vars(word_expr, state);
        let case_word = Self::strip_quotes(word_expanded.trim());

        let mut depth = 1usize;
        let mut esac_index = None;
        let mut i = start + 1;
        while i < end {
            let line = Self::normalize_script_line(&lines[i]);
            if line.starts_with("case ") {
                depth += 1;
            } else if line == "esac" {
                depth -= 1;
                if depth == 0 {
                    esac_index = Some(i);
                    break;
                }
            }
            i += 1;
        }

        let esac = esac_index
            .ok_or_else(|| ShellError::InvalidCommand("case block missing 'esac'".to_string()))?;

        let mut cursor = start + 1;
        let mut matched = false;
        while cursor < esac {
            let line = Self::normalize_script_line(&lines[cursor]);
            if line.is_empty() || line.starts_with('#') {
                cursor += 1;
                continue;
            }

            if let Some(close_paren) = line.find(')') {
                let patterns = line[..close_paren].trim();
                let remainder = line[close_paren + 1..].trim();
                let is_match = !matched && self.case_pattern_matches(patterns, &case_word);

                if remainder.ends_with(";;") {
                    if is_match {
                        let cmd = remainder.trim_end_matches(";;").trim();
                        if !cmd.is_empty() {
                            self.execute_script_simple_line(cmd, state)?;
                        }
                        matched = true;
                    }
                    cursor += 1;
                    continue;
                }

                cursor += 1;
                while cursor < esac {
                    let branch_line = Self::normalize_script_line(&lines[cursor]);
                    if branch_line.ends_with(";;") {
                        if is_match {
                            let cmd = branch_line.trim_end_matches(";;").trim();
                            if !cmd.is_empty() {
                                self.execute_script_simple_line(cmd, state)?;
                            }
                            matched = true;
                        }
                        break;
                    }

                    if is_match {
                        self.execute_script_simple_line(&branch_line, state)?;
                    }
                    cursor += 1;
                }
            }
            cursor += 1;
        }

        Ok(esac + 1)
    }

    fn execute_if_block(
        &mut self,
        lines: &[String],
        start: usize,
        end: usize,
        state: &mut ScriptState,
    ) -> Result<usize> {
        let mut depth = 0usize;
        let mut fi_index = None;
        let mut i = start;
        while i < end {
            let line = Self::normalize_script_line(&lines[i]);
            if line.starts_with("if ") {
                depth += 1;
            } else if line == "fi" {
                depth -= 1;
                if depth == 0 {
                    fi_index = Some(i);
                    break;
                }
            }
            i += 1;
        }

        let fi = fi_index
            .ok_or_else(|| ShellError::InvalidCommand("if block missing 'fi'".to_string()))?;

        let mut branches: Vec<(String, usize, usize)> = Vec::new();
        let mut else_block: Option<(usize, usize)> = None;
        let mut cursor = start;
        let mut nesting = 0usize;
        while cursor <= fi {
            let line = Self::normalize_script_line(&lines[cursor]);
            if line.starts_with("if ") {
                nesting += 1;
                if nesting == 1 {
                    let cond = Self::extract_if_condition(&line, "if")?;
                    let (body_start, next) = self.find_then_body_start(lines, cursor, fi)?;
                    let body_end =
                        self.find_if_branch_end(lines, body_start, fi, &["elif", "else", "fi"])?;
                    branches.push((cond, body_start, body_end));
                    cursor = next.max(body_end);
                    continue;
                }
            } else if line == "fi" {
                nesting = nesting.saturating_sub(1);
            } else if nesting == 1 && line.starts_with("elif ") {
                let cond = Self::extract_if_condition(&line, "elif")?;
                let (body_start, next) = self.find_then_body_start(lines, cursor, fi)?;
                let body_end =
                    self.find_if_branch_end(lines, body_start, fi, &["elif", "else", "fi"])?;
                branches.push((cond, body_start, body_end));
                cursor = next.max(body_end);
                continue;
            } else if nesting == 1 && line == "else" {
                else_block = Some((cursor + 1, fi));
                break;
            }
            cursor += 1;
        }

        let mut executed = false;
        for (condition, body_start, body_end) in branches {
            let expanded_condition = self.expand_script_vars(&condition, state);
            self.execute_command(&expanded_condition)?;
            if self.last_exit_code == 0 {
                let _ = self.execute_script_lines(lines, body_start, body_end, state)?;
                executed = true;
                break;
            }
        }

        if !executed {
            if let Some((body_start, body_end)) = else_block {
                let _ = self.execute_script_lines(lines, body_start, body_end, state)?;
            }
        }

        Ok(fi + 1)
    }

    fn extract_if_condition(line: &str, keyword: &str) -> Result<String> {
        let raw = line.trim_start_matches(keyword).trim();
        let cond = raw
            .trim_end_matches("; then")
            .trim_end_matches(" then")
            .trim_end_matches(';')
            .trim();
        if cond.is_empty() {
            return Err(ShellError::InvalidCommand(format!(
                "{} condition cannot be empty",
                keyword
            )));
        }
        Ok(cond.to_string())
    }

    fn find_then_body_start(&self, lines: &[String], start: usize, fi: usize) -> Result<(usize, usize)> {
        let header = Self::normalize_script_line(&lines[start]);
        if header.ends_with("; then") || header.ends_with(" then") {
            return Ok((start + 1, start + 1));
        }
        let mut i = start + 1;
        while i <= fi {
            let line = Self::normalize_script_line(&lines[i]);
            if line == "then" {
                return Ok((i + 1, i + 1));
            }
            i += 1;
        }
        Err(ShellError::InvalidCommand(
            "if syntax expects 'then'".to_string(),
        ))
    }

    fn find_if_branch_end(
        &self,
        lines: &[String],
        start: usize,
        fi: usize,
        branch_markers: &[&str],
    ) -> Result<usize> {
        let mut depth = 1usize;
        let mut i = start;
        while i <= fi {
            let line = Self::normalize_script_line(&lines[i]);
            if line.starts_with("if ") {
                depth += 1;
            } else if line == "fi" {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            } else if depth == 1 && branch_markers.iter().any(|m| line == *m || line.starts_with(&format!("{m} "))) {
                return Ok(i);
            }
            i += 1;
        }
        Err(ShellError::InvalidCommand(
            "if branch termination not found".to_string(),
        ))
    }

    fn parse_function_header(line: &str) -> Option<String> {
        let trimmed = line.trim();
        if !trimmed.ends_with('{') {
            return None;
        }
        let header = trimmed.trim_end_matches('{').trim();
        if let Some(name) = header.strip_suffix("()") {
            let n = name.trim();
            if !n.is_empty() {
                return Some(n.to_string());
            }
        }
        None
    }

    fn register_script_function(
        &mut self,
        lines: &[String],
        start: usize,
        end: usize,
        func_name: String,
        state: &mut ScriptState,
    ) -> Result<usize> {
        let mut depth = 1usize;
        let mut cursor = start + 1;
        let mut body: Vec<String> = Vec::new();
        while cursor < end {
            let line = Self::normalize_script_line(&lines[cursor]);
            if line.ends_with('{') {
                depth += 1;
            } else if line == "}" {
                depth -= 1;
                if depth == 0 {
                    state.functions.insert(func_name, body);
                    return Ok(cursor + 1);
                }
            }
            if depth > 0 {
                body.push(line);
            }
            cursor += 1;
        }

        Err(ShellError::InvalidCommand(
            "function block missing '}'".to_string(),
        ))
    }

    fn case_pattern_matches(&self, patterns: &str, value: &str) -> bool {
        for pattern in patterns.split('|') {
            let pat = Self::strip_quotes(pattern.trim());
            if pat == "*" {
                return true;
            }
            if pat.contains('*') || pat.contains('?') || pat.contains('[') {
                if let Ok(glob_pattern) = glob::Pattern::new(&pat) {
                    if glob_pattern.matches(value) {
                        return true;
                    }
                }
            } else if pat == value {
                return true;
            }
        }
        false
    }

    fn execute_script_simple_line(&mut self, line: &str, state: &mut ScriptState) -> Result<()> {
        let trimmed = line.trim();
        if trimmed.starts_with("unset -f ") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() > 2 {
                state.functions.remove(parts[2]);
            }
            return Ok(());
        }

        if trimmed.starts_with("shift") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let shift_n = if parts.len() > 1 {
                parts[1].parse::<usize>().unwrap_or(1)
            } else {
                1
            };
            state.shift(shift_n);
            return Ok(());
        }

        let expanded = self.expand_script_vars(trimmed, state);
        if expanded.is_empty() {
            return Ok(());
        }

        let parts: Vec<&str> = expanded.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        let mut idx = 0usize;
        while idx < parts.len() && Self::is_assignment_token(parts[idx]) {
            if let Some((key, value)) = parts[idx].split_once('=') {
                let mut normalized_value = Self::normalize_assignment_value(value);
                if key.eq_ignore_ascii_case("PATH") {
                    normalized_value = Self::normalize_path_list_for_windows(&normalized_value);
                }

                state
                    .locals
                    .insert(key.to_string(), normalized_value.to_string());
                self.env_vars
                    .insert(key.to_string(), ArrayValue::String(normalized_value.clone()));
                std::env::set_var(key, &normalized_value);
            }
            idx += 1;
        }

        if idx >= parts.len() {
            return Ok(());
        }

        if let Some(func_body) = state.functions.get(parts[idx]).cloned() {
            let _ = self.execute_script_lines(&func_body, 0, func_body.len(), state)?;
            return Ok(());
        }

        let command = parts[idx..].join(" ");
        self.execute_command(&command)
    }

    fn is_assignment_token(token: &str) -> bool {
        if let Some((key, _)) = token.split_once('=') {
            if key.is_empty() {
                return false;
            }
            let mut chars = key.chars();
            if let Some(first) = chars.next() {
                if !(first.is_ascii_alphabetic() || first == '_') {
                    return false;
                }
            }
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        } else {
            false
        }
    }

    fn expand_script_vars(&self, input: &str, state: &ScriptState) -> String {
        let mut out = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch != '$' {
                out.push(ch);
                continue;
            }

            let Some(next) = chars.peek().copied() else {
                out.push('$');
                break;
            };

            match next {
                '{' => {
                    chars.next();
                    let mut name = String::new();
                    while let Some(c) = chars.peek().copied() {
                        chars.next();
                        if c == '}' {
                            break;
                        }
                        name.push(c);
                    }
                    if let Some((var_name, default_value)) = name.split_once(":-") {
                        let value = self.resolve_script_var(var_name.trim(), state);
                        if value.is_empty() {
                            out.push_str(default_value);
                        } else {
                            out.push_str(&value);
                        }
                    } else {
                        out.push_str(&self.resolve_script_var(&name, state));
                    }
                }
                '#' => {
                    chars.next();
                    out.push_str(&state.positional.len().to_string());
                }
                '@' | '*' => {
                    chars.next();
                    out.push_str(&state.positional.join(" "));
                }
                c if c.is_ascii_digit() => {
                    let mut index = String::new();
                    while let Some(d) = chars.peek().copied() {
                        if d.is_ascii_digit() {
                            chars.next();
                            index.push(d);
                        } else {
                            break;
                        }
                    }
                    let idx = index.parse::<usize>().unwrap_or(0);
                    if idx > 0 {
                        if let Some(value) = state.positional(idx - 1) {
                            out.push_str(value);
                        }
                    }
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    let mut name = String::new();
                    while let Some(c2) = chars.peek().copied() {
                        if c2.is_ascii_alphanumeric() || c2 == '_' {
                            chars.next();
                            name.push(c2);
                        } else {
                            break;
                        }
                    }
                    out.push_str(&self.resolve_script_var(&name, state));
                }
                _ => out.push('$'),
            }
        }

        out
    }

    fn resolve_script_var(&self, name: &str, state: &ScriptState) -> String {
        if let Some(value) = state.locals.get(name) {
            return value.clone();
        }

        if let Some(value) = self.env_vars.get(name) {
            if let Some(s) = value.as_string() {
                return s.to_string();
            }
        }

        if let Some((_, value)) = self
            .env_vars
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
        {
            if let Some(s) = value.as_string() {
                return s.to_string();
            }
        }

        String::new()
    }

    fn normalize_script_line(line: &str) -> String {
        let trimmed = line
            .trim_matches(|c: char| c == '\u{feff}' || c == '\u{fffe}' || c.is_whitespace());
        Self::strip_inline_comment(trimmed).trim().to_string()
    }

    fn strip_inline_comment(line: &str) -> String {
        let mut out = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;

        for ch in line.chars() {
            if escaped {
                out.push(ch);
                escaped = false;
                continue;
            }

            if ch == '\\' && !in_single {
                out.push(ch);
                escaped = true;
                continue;
            }

            if ch == '\'' && !in_double {
                in_single = !in_single;
                out.push(ch);
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                out.push(ch);
                continue;
            }

            if ch == '#' && !in_single && !in_double {
                break;
            }

            out.push(ch);
        }

        out
    }

    fn strip_quotes(s: &str) -> String {
        if s.len() >= 2 {
            let bytes = s.as_bytes();
            if (bytes[0] == b'"' && bytes[s.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[s.len() - 1] == b'\'')
            {
                return s[1..s.len() - 1].to_string();
            }
        }
        s.to_string()
    }

    fn normalize_assignment_value(raw: &str) -> String {
        let mut out = String::new();
        let mut in_single = false;
        let mut in_double = false;

        for ch in raw.chars() {
            if ch == '\'' && !in_double {
                in_single = !in_single;
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                continue;
            }

            out.push(ch);
        }

        out
    }

    fn normalize_path_list_for_windows(path_value: &str) -> String {
        if cfg!(not(windows)) || path_value.is_empty() {
            return path_value.to_string();
        }

        if path_value.contains(';') {
            return path_value.to_string();
        }

        let chars: Vec<char> = path_value.chars().collect();
        let mut parts: Vec<String> = Vec::new();
        let mut current = String::new();

        for i in 0..chars.len() {
            let ch = chars[i];
            if ch == ':' {
                let prev = if i > 0 { Some(chars[i - 1]) } else { None };
                let next = if i + 1 < chars.len() {
                    Some(chars[i + 1])
                } else {
                    None
                };
                let is_drive_colon = prev.map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
                    && next.map(|c| c == '\\' || c == '/').unwrap_or(false);
                if !is_drive_colon {
                    if !current.is_empty() {
                        parts.push(current.clone());
                        current.clear();
                    } else {
                        parts.push(String::new());
                    }
                    continue;
                }
            }
            current.push(ch);
        }

        if !current.is_empty() {
            parts.push(current);
        }

        parts.join(";")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_shell_creation() {
        let shell = Shell::new(false);
        assert!(shell.is_ok());
        let shell = shell.unwrap();
        assert_eq!(shell.current_dir, std::env::current_dir().unwrap());
    }

    #[test]
    fn test_get_env_var() {
        let shell = Shell::new(false).unwrap();
        let value = shell.get_env_var("USERNAME", "default");
        assert!(value != "default" || std::env::var("USERNAME").is_err());
    }

    #[test]
    fn test_and_short_circuit_on_failure() {
        let mut shell = Shell::new(false).unwrap();
        shell.env_vars.remove("SHOULD_NOT_RUN");
        shell
            .execute_command("notexistcmd && set SHOULD_NOT_RUN=1")
            .unwrap();
        assert!(shell.env_vars.get("SHOULD_NOT_RUN").is_none());
        assert_ne!(shell.last_exit_code, 0);
    }

    #[test]
    fn test_or_runs_on_failure() {
        let mut shell = Shell::new(false).unwrap();
        shell.env_vars.remove("SHOULD_RUN");
        shell
            .execute_command("notexistcmd || set SHOULD_RUN=1")
            .unwrap();
        assert_eq!(
            shell.env_vars.get("SHOULD_RUN"),
            Some(&ArrayValue::String("1".to_string()))
        );
        assert_eq!(shell.last_exit_code, 0);
    }

    #[test]
    fn test_or_short_circuit_on_success() {
        let mut shell = Shell::new(false).unwrap();
        shell.env_vars.remove("SHOULD_NOT_RUN_OR");
        shell
            .execute_command("echo hi || set SHOULD_NOT_RUN_OR=1")
            .unwrap();
        assert!(shell.env_vars.get("SHOULD_NOT_RUN_OR").is_none());
        assert_eq!(shell.last_exit_code, 0);
    }

    #[test]
    fn test_builtin_echo_redirection() {
        let mut shell = Shell::new(false).unwrap();
        let test_dir = std::env::temp_dir().join(format!("winuxsh_redir_{}", std::process::id()));
        fs::create_dir_all(&test_dir).unwrap();
        let out_path = test_dir.join("out.txt");
        let cmd = format!("echo hello > {}", out_path.to_string_lossy());
        shell.execute_command(&cmd).unwrap();
        let out = fs::read_to_string(out_path).unwrap();
        assert_eq!(out, "hello\n");
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn test_script_control_flow_and_positional_args() {
        let mut shell = Shell::new(false).unwrap();
        let test_dir = std::env::temp_dir().join(format!("winuxsh_script_{}", std::process::id()));
        fs::create_dir_all(&test_dir).unwrap();

        let script_path = test_dir.join("verify.sh");
        let out_path = test_dir.join("out.txt");
        let script = "\
#!/bin/bash
while true; do
  echo loop
  break
done
RUN_CMD=echo
case x in
  x) echo ok ;;
  *) echo bad ;;
esac
echo first=$1
shift
echo second=$1
echo redirected > __OUT_PATH__
";
        let script = script.replace("__OUT_PATH__", &out_path.to_string_lossy());
        fs::write(&script_path, script).unwrap();

        let args = vec!["A".to_string(), "B".to_string()];
        shell.run_script_file(&script_path, &args).unwrap();

        let out = fs::read_to_string(out_path).unwrap();
        assert_eq!(out, "redirected\n");
        assert_eq!(
            shell.env_vars.get("RUN_CMD"),
            Some(&ArrayValue::String("echo".to_string()))
        );
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn test_expand_wildcards_keeps_unmatched_pattern_per_argument() {
        let shell = Shell::new(false).unwrap();
        let unmatched = format!(
            "winuxsh_unmatched_{}_*.unlikely",
            std::process::id()
        );
        let args = vec!["Cargo.*".to_string(), unmatched.clone()];

        let expanded = shell.expand_wildcards(&args);

        assert!(expanded.iter().any(|arg| arg.ends_with("Cargo.toml")));
        assert!(expanded.contains(&unmatched));
    }

    #[test]
    fn test_builtin_stderr_redirection_file_materialized() {
        let mut shell = Shell::new(false).unwrap();
        let test_dir = std::env::temp_dir().join(format!("winuxsh_stderr_{}", std::process::id()));
        fs::create_dir_all(&test_dir).unwrap();
        let err_path = test_dir.join("err.txt");
        let cmd = format!("echo hello 2> {}", err_path.to_string_lossy());
        shell.execute_command(&cmd).unwrap();
        let err = fs::read_to_string(err_path).unwrap();
        assert_eq!(err, "");
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn test_command_substitution_builtin_pwd_uses_shell_path() {
        let mut shell = Shell::new(false).unwrap();
        let output = shell.execute_substitution_command("pwd");
        assert_eq!(output.trim(), shell.current_dir.display().to_string());
    }

    #[test]
    fn test_command_substitution_or_uses_shell_execution() {
        let mut shell = Shell::new(false).unwrap();
        let output = shell.execute_substitution_command("notexistcmd || echo fallback");
        assert_eq!(output.trim(), "fallback");
    }

    #[test]
    fn test_command_substitution_sequence_uses_shell_execution() {
        let mut shell = Shell::new(false).unwrap();
        let output = shell.execute_substitution_command("echo one; echo two");
        assert_eq!(output.trim(), "one\ntwo");
    }

    #[test]
    fn test_command_substitution_preserves_side_effects() {
        let mut shell = Shell::new(false).unwrap();
        let _ = shell.execute_substitution_command("set SUBST_VAR=1");
        assert_eq!(
            shell.env_vars.get("SUBST_VAR"),
            Some(&ArrayValue::String("1".to_string()))
        );
    }

    #[test]
    fn test_command_substitution_failure_returns_empty_string() {
        let mut shell = Shell::new(false).unwrap();
        let output = shell.execute_substitution_command("notexistcmd");
        assert_eq!(output, "");
    }

    #[test]
    fn test_command_substitution_pipeline_captures_last_stdout() {
        let mut shell = Shell::new(false).unwrap();
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        let cmd_exe = format!("{system_root}\\System32\\cmd.exe");
        let findstr_exe = format!("{system_root}\\System32\\findstr.exe");
        let command = format!("{cmd_exe} /C echo hello | {findstr_exe} h");
        let output = shell.execute_substitution_command(&command);
        assert_eq!(output.trim(), "hello");
    }

    #[test]
    fn test_builtin_stdout_to_stderr_file() {
        let mut shell = Shell::new(false).unwrap();
        let test_dir =
            std::env::temp_dir().join(format!("winuxsh_stdout_to_stderr_{}", std::process::id()));
        fs::create_dir_all(&test_dir).unwrap();
        let err_path = test_dir.join("err.txt");
        let cmd = format!("echo redirected 1>&2 2> {}", err_path.to_string_lossy());
        shell.execute_command(&cmd).unwrap();
        let err = fs::read_to_string(err_path).unwrap();
        assert_eq!(err, "redirected\n");
        let _ = fs::remove_dir_all(test_dir);
    }
}
