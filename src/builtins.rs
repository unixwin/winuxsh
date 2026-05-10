// Built-in commands for WinSH
use crate::array::ArrayValue;
use crate::error::{Result, ShellError};
use crate::job::JobStatus;
use crate::oh_my_winuxsh::OhMyWinuxsh;
use crate::plugin::Plugin;
use crate::shell::Shell;
use crate::winuxcmd_ffi::WinuxCmdFFI;
use colored::Colorize;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Built-in command handler
impl Shell {
    /// Handle built-in commands
    pub fn handle_builtin(&mut self, args: &[String]) -> Option<Result<()>> {
        if args.is_empty() {
            return Some(Ok(()));
        }

        match args[0].as_str() {
            "cd" => {
                let dir_str = if args.len() > 1 {
                    args[1].clone()
                } else {
                    dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .to_str()
                        .unwrap()
                        .to_string()
                };

                let new_dir = if dir_str == "~" {
                    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
                } else {
                    PathBuf::from(dir_str.as_str())
                };

                if let Err(e) = std::env::set_current_dir(&new_dir) {
                    return Some(Err(ShellError::InvalidCommand(format!(
                        "cd: {} - {}",
                        dir_str, e
                    ))));
                }

                self.current_dir = std::env::current_dir().unwrap();
                Some(Ok(()))
            }
            "pwd" => {
                println!("{}", self.current_dir.display());
                Some(Ok(()))
            }
            "echo" => {
                let output = args[1..].join(" ");
                println!("{}", output);
                Some(Ok(()))
            }
            "true" => Some(Ok(())),
            "exit" | "quit" => {
                std::process::exit(0);
            }
            "clear" | "cls" => {
                print!("\x1b[2J\x1b[H");
                std::io::stdout().flush().unwrap();
                Some(Ok(()))
            }
            "set" => {
                if args.len() > 1 {
                    let arg = &args[1];
                    if arg.contains('=') {
                        if let Some((key, value)) = arg.split_once('=') {
                            let normalized = Self::strip_assignment_quotes(value);
                            self.env_vars
                                .insert(key.to_string(), ArrayValue::String(normalized));
                        }
                    }
                }
                Some(Ok(()))
            }
            "export" => {
                if args.len() > 1 {
                    let arg = &args[1];
                    if arg.contains('=') {
                        if let Some((key, value)) = arg.split_once('=') {
                            let normalized = Self::strip_assignment_quotes(value);
                            self.env_vars
                                .insert(key.to_string(), ArrayValue::String(normalized.clone()));
                            std::env::set_var(key, normalized);
                        }
                    }
                }
                Some(Ok(()))
            }
            "unset" => {
                if args.len() > 1 {
                    if args[1] == "-f" {
                        // Function unsetting is handled by script runtime.
                    } else {
                        self.env_vars.remove(&args[1]);
                        std::env::remove_var(&args[1]);
                    }
                }
                Some(Ok(()))
            }
            "hash" => {
                // POSIX shell compatibility: `hash -r` can be treated as no-op.
                Some(Ok(()))
            }
            "[" | "test" => {
                let is_bracket = args[0] == "[";
                let slice_end = if is_bracket && args.last().map(|s| s.as_str()) == Some("]") {
                    args.len() - 1
                } else {
                    args.len()
                };
                let test_args = if is_bracket {
                    &args[1..slice_end]
                } else {
                    &args[1..]
                };

                let ok = Self::eval_test_expr(test_args);
                if ok {
                    Some(Ok(()))
                } else {
                    Some(Err(ShellError::InvalidCommand(
                        "test expression evaluated to false".to_string(),
                    )))
                }
            }
            "env" => {
                for (key, value) in &self.env_vars {
                    match value {
                        ArrayValue::String(v) => {
                            println!("{}={}", key, v);
                        }
                        ArrayValue::Array(arr) => {
                            println!("{}=({})", key, arr.join(" "));
                        }
                    }
                }
                Some(Ok(()))
            }
            "help" => {
                self.print_help();
                Some(Ok(()))
            }
            "history" => {
                self.print_history();
                Some(Ok(()))
            }
            "alias" => {
                if args.len() == 1 {
                    for (name, value) in &self.aliases {
                        println!("{}='{}'", name, value);
                    }
                } else if args.len() > 1 {
                    if let Some((name, value)) = args[1].split_once('=') {
                        self.aliases.insert(name.to_string(), value.to_string());
                    }
                }
                Some(Ok(()))
            }
            "unalias" => {
                if args.len() > 1 {
                    self.aliases.remove(&args[1]);
                }
                Some(Ok(()))
            }
            "source" | "." => {
                if args.len() < 2 {
                    return Some(Err(ShellError::InvalidCommand(
                        "source: filename argument required".to_string(),
                    )));
                }

                let script_arg = &args[1];
                let script_path = if script_arg == "~" {
                    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
                } else {
                    let raw = PathBuf::from(script_arg);
                    if raw.is_absolute() {
                        raw
                    } else {
                        self.current_dir.join(raw)
                    }
                };

                let script_args: Vec<String> = if args.len() > 2 {
                    args[2..].to_vec()
                } else {
                    Vec::new()
                };

                if let Err(e) = self.run_script_file(&script_path, &script_args) {
                    return Some(Err(ShellError::InvalidCommand(format!(
                        "source: failed to execute '{}': {}",
                        script_path.display(),
                        e
                    ))));
                }
                Some(Ok(()))
            }
            "array" => {
                self.handle_array_command(&args[1..]);
                Some(Ok(()))
            }
            "jobs" => {
                self.handle_jobs_command();
                Some(Ok(()))
            }
            "fg" => {
                self.handle_fg_command(&args[1..]);
                Some(Ok(()))
            }
            "bg" => {
                self.handle_bg_command(&args[1..]);
                Some(Ok(()))
            }
            "plugin" => {
                self.handle_plugin_command(&args[1..]);
                Some(Ok(()))
            }
            "theme" => {
                self.handle_theme_command(&args[1..]);
                Some(Ok(()))
            }
            "oh-my-winuxsh" => {
                // Handle oh-my-winuxsh plugin commands directly
                let plugin = OhMyWinuxsh;
                if let Err(e) = plugin.execute(&args[1..], self) {
                    println!("Error executing oh-my-winuxsh command: {}", e);
                }
                Some(Ok(()))
            }
            "ffi_test" => {
                self.handle_ffi_test(&args[1..]);
                Some(Ok(()))
            }
            "ffi_version" => {
                self.handle_ffi_version();
                Some(Ok(()))
            }
            "ffi_commands" => {
                self.handle_ffi_commands();
                Some(Ok(()))
            }
            _ => None,
        }
    }

    fn eval_test_expr(args: &[String]) -> bool {
        if args.is_empty() {
            return false;
        }

        if args.len() == 2 && args[0] == "-n" {
            return !args[1].is_empty();
        }
        if args.len() == 2 && args[0] == "-z" {
            return args[1].is_empty();
        }
        if args.len() == 3 && args[1] == "=" {
            return args[0] == args[2];
        }
        if args.len() == 3 && args[1] == "!=" {
            return args[0] != args[2];
        }

        // Fallback: non-empty single argument is true.
        if args.len() == 1 {
            return !args[0].is_empty();
        }

        false
    }

    fn strip_assignment_quotes(value: &str) -> String {
        if value.len() >= 2 {
            let bytes = value.as_bytes();
            if (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
                || (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            {
                return value[1..value.len() - 1].to_string();
            }
        }
        value.to_string()
    }

    /// Handle array commands
    fn handle_array_command(&mut self, args: &[String]) {
        if args.is_empty() {
            println!("Array commands: define, get, len, list");
            return;
        }

        match args[0].as_str() {
            "define" => {
                if args.len() > 2 {
                    let array_name = &args[1];
                    let elements: Vec<String> = args[2..].to_vec();
                    self.env_vars
                        .insert(array_name.to_string(), ArrayValue::Array(elements));
                    println!(
                        "Array '{}' defined with {} elements",
                        array_name,
                        args.len() - 2
                    );
                }
            }
            "get" => {
                if args.len() > 2 {
                    let array_name = &args[1];
                    let index: usize = args[2].parse().unwrap_or(0);
                    if let Some(ArrayValue::Array(arr)) = self.env_vars.get(array_name) {
                        if let Some(element) = arr.get(index) {
                            println!("{}", element);
                        } else {
                            println!("Index out of bounds");
                        }
                    } else {
                        println!("Array '{}' not found", array_name);
                    }
                }
            }
            "len" => {
                if args.len() > 1 {
                    let array_name = &args[1];
                    if let Some(ArrayValue::Array(arr)) = self.env_vars.get(array_name) {
                        println!("{}", arr.len());
                    } else {
                        println!("Array '{}' not found", array_name);
                    }
                }
            }
            "list" => {
                for (key, value) in &self.env_vars {
                    if let ArrayValue::Array(arr) = value {
                        println!("{}=({})", key, arr.join(" "));
                    }
                }
            }
            _ => {
                println!("Array commands: define, get, len, list");
            }
        }
    }

    /// Handle jobs command
    fn handle_jobs_command(&self) {
        if !self.job_manager.has_jobs() {
            println!("{}", "No background jobs".cyan());
            return;
        }

        println!("{}", "Background jobs:".cyan());
        println!("{}  {}  {}", "ID".cyan(), "PID".cyan(), "Command".cyan());

        for job in self.job_manager.list_jobs() {
            let status_str = match job.status {
                JobStatus::Running => "Running".green(),
                JobStatus::Stopped => "Stopped".yellow(),
                JobStatus::Done => "Done".blue(),
            };

            println!(
                "{}  {}  {}  {}",
                format!("[{}]", job.id).cyan(),
                job.pid,
                status_str,
                job.command
            );
        }
    }

    /// Handle fg command
    fn handle_fg_command(&mut self, args: &[String]) {
        if args.is_empty() {
            eprintln!("{} {}", "fg:".red(), "Job number required");
            return;
        }

        let job_id = if args[0].starts_with('%') {
            args[0][1..].parse::<u32>()
        } else {
            args[0].parse::<u32>()
        };

        let job_id = match job_id {
            Ok(id) => id,
            Err(_) => {
                eprintln!(
                    "{} {}",
                    "fg:".red(),
                    format!("Invalid job number '{}'", args[0])
                );
                return;
            }
        };

        let job_index = match self.job_manager.find_job_index(job_id) {
            Some(index) => index,
            None => {
                eprintln!("{} {}", "fg:".red(), format!("Job %{} not found", job_id));
                return;
            }
        };

        let job = self.job_manager.get_job(job_id).unwrap();
        println!("{} {}", "Continuing job:".cyan(), format!("[{}]", job.id));

        // Remove job from list (simplified implementation)
        let _ = self.job_manager.remove_job(job_index);
    }

    /// Handle bg command
    fn handle_bg_command(&mut self, args: &[String]) {
        if args.is_empty() {
            eprintln!("{} {}", "bg:".red(), "Job number required");
            return;
        }

        let job_id = if args[0].starts_with('%') {
            args[0][1..].parse::<u32>()
        } else {
            args[0].parse::<u32>()
        };

        let job_id = match job_id {
            Ok(id) => id,
            Err(_) => {
                eprintln!(
                    "{} {}",
                    "bg:".red(),
                    format!("Invalid job number '{}'", args[0])
                );
                return;
            }
        };

        let _job_index = match self.job_manager.find_job_index(job_id) {
            Some(index) => index,
            None => {
                eprintln!("{} {}", "bg:".red(), format!("Job %{} not found", job_id));
                return;
            }
        };

        if let Some(job) = self.job_manager.get_job_mut(job_id) {
            job.set_status(JobStatus::Running);
            println!(
                "{} {}",
                "Continue background job:".cyan(),
                format!("[{}]", job.id)
            );
        }
    }

    /// Handle plugin command
    fn handle_plugin_command(&self, args: &[String]) {
        if args.is_empty() {
            println!("Plugin commands: list, load");
            return;
        }

        match args[0].as_str() {
            "list" => {
                println!("{}", "Loaded plugins:".cyan());
                for plugin_name in self.plugins.list_plugins() {
                    println!("  - {}", plugin_name);
                }
                if self.plugins.plugin_count() == 0 {
                    println!("  (No plugins loaded)");
                }
            }
            "load" => {
                if args.len() > 1 {
                    println!("Plugin '{}' not found (not implemented yet)", args[1]);
                }
            }
            _ => {
                println!("Plugin commands: list, load");
            }
        }
    }

    /// Print help information
    fn print_help(&self) {
        println!("{}", "WinSH MVP6 - Available commands:".green());
        println!();
        println!("{}", "Built-in commands:".cyan());
        println!("  cd [dir]       - Change directory");
        println!("  pwd            - Print current directory");
        println!("  echo [text]    - Print text (supports env vars)");
        println!("  set VAR=VALUE  - Set environment variable");
        println!("  export VAR=VALUE - Set environment variable");
        println!("  unset VAR      - Remove environment variable");
        println!("  env            - Display all environment variables");
        println!("  source [file] [args...] - Execute script in current shell");
        println!("  . [file] [args...]      - Alias for source");
        println!("  exit           - Exit shell");
        println!("  quit           - Exit shell");
        println!("  clear          - Clear screen");
        println!("  cls            - Clear screen");
        println!("  alias [name=value] - Display or set alias");
        println!("  unalias [name]  - Remove alias");
        println!("  help           - Display help information");
        println!("  history        - Display command history");
        println!("  jobs           - List background jobs");
        println!("  fg [job_id]    - Bring job to foreground");
        println!("  bg [job_id]    - Resume stopped job in background");
        println!();
        println!("{}", "Array support:".cyan());
        println!("  array define name elem1 elem2 ... - Define array");
        println!("  array get name index            - Get array element");
        println!("  array len name                  - Get array length");
        println!("  array list                      - List all arrays");
        println!();
        println!("{}", "Plugin system:".cyan());
        println!("  plugin list   - List loaded plugins");
        println!("  plugin load   - Load plugin (not implemented yet)");
        println!();
        println!("{}", "Theme system:".cyan());
        println!("  theme list              - List all available themes");
        println!("  theme set <name>        - Set a theme");
        println!("  theme current           - Show current theme");
        println!("  theme preview <name>    - Preview a theme");
        println!();
        println!("{}", "Oh-My-Winuxsh:".cyan());
        println!("  oh-my-winuxsh              - Show oh-my-winuxsh help");
        println!("  oh-my-winuxsh version       - Show version information");
        println!("  oh-my-winuxsh list-themes  - List all available themes");
        println!("  oh-my-winuxsh set-theme <name> - Change current theme");
        println!("  oh-my-winuxsh current-theme - Show current theme");
        println!();
        println!("{}", "FFI (WinuxCmd):".cyan());
        println!("  ffi_test [cmd] [args] - Test WinuxCmd FFI execution");
        println!("  ffi_version            - Show WinuxCmd version");
        println!("  ffi_commands           - List all available commands");
        println!();
        println!("{}", "Available themes:".cyan());
        println!("  default, dark, light, colorful, minimal, cyberpunk, ocean, forest");
    }

    /// Print command history
    fn print_history(&self) {
        if let Ok(history) = std::fs::read_to_string(&self.history_path) {
            let lines: Vec<String> = history
                .lines()
                .map(|l| {
                    l.trim_matches(|c: char| {
                        c == '\u{feff}' || c == '\u{fffe}' || c.is_whitespace()
                    })
                    .to_string()
                })
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();

            println!("{}", "Command History:".cyan());
            for (i, line) in lines.iter().enumerate() {
                println!("  {}  {}", i + 1, line);
            }
        } else {
            println!("{} {}", "Warning:".yellow(), "No history available");
        }
    }

    /// Handle theme command
    fn handle_theme_command(&mut self, args: &[String]) {
        let theme_plugin = self.theme_plugin.clone();
        let result = theme_plugin.execute(args, self);
        if let Err(e) = result {
            eprintln!("{} {}", "Theme error:".red(), e);
        }
    }

    /// Handle FFI test command
    fn handle_ffi_test(&mut self, args: &[String]) {
        use crate::winuxcmd_ffi::WinuxCmdFFI;

        println!("{}", "WinuxCmd FFI Test".cyan());
        println!("{}", "================".cyan());

        // Initialize FFI if needed
        if let Err(e) = WinuxCmdFFI::init() {
            eprintln!("{} {}", "FFI initialization failed:".red(), e);
            return;
        }

        if !WinuxCmdFFI::is_initialized() {
            eprintln!(
                "{} {}",
                "FFI not available:".yellow(),
                "Initialization failed"
            );
            return;
        }

        let command = if args.len() > 0 {
            args[0].clone()
        } else {
            "pwd".to_string()
        };

        let args_slice: Vec<String> = if args.len() > 1 {
            args[1..].to_vec()
        } else {
            vec![]
        };

        println!("Executing: {} {:?}", command, args_slice);
        println!();

        match WinuxCmdFFI::execute(&command, &args_slice) {
            Ok(response) => {
                if !response.stdout.is_empty() {
                    let stdout_str = String::from_utf8_lossy(&response.stdout);
                    print!("{}", stdout_str);
                }
                if !response.stderr.is_empty() {
                    let stderr_str = String::from_utf8_lossy(&response.stderr);
                    eprint!("{} {}", "Error:".red(), stderr_str);
                }
                println!("Exit code: {}", response.exit_code);
            }
            Err(e) => {
                eprintln!("{} {}", "FFI error:".red(), e);
            }
        }
    }

    /// Handle FFI version command
    fn handle_ffi_version(&self) {
        use crate::winuxcmd_ffi::WinuxCmdFFI;

        let _ = WinuxCmdFFI::init();
        if WinuxCmdFFI::is_initialized() {
            match WinuxCmdFFI::get_version() {
                Ok(version) => println!("{} {}", "WinuxCmd version:".green(), version),
                Err(e) => eprintln!("{} {}", "Failed to get version:".yellow(), e),
            }
        } else {
            eprintln!(
                "{} {}",
                "FFI not available:".yellow(),
                "Initialization failed"
            );
        }
    }

    /// Handle FFI commands list command
    fn handle_ffi_commands(&self) {
        use crate::winuxcmd_ffi::WinuxCmdFFI;

        let _ = WinuxCmdFFI::init();
        if WinuxCmdFFI::is_initialized() {
            match WinuxCmdFFI::get_all_commands() {
                Ok(commands) => {
                    println!(
                        "{} {} available commands",
                        "WinuxCmd".cyan(),
                        commands.len()
                    );
                    println!("{}", "=================".cyan());
                    for (i, cmd) in commands.iter().take(20).enumerate() {
                        println!("  {:2}. {}", i + 1, cmd);
                    }
                    if commands.len() > 20 {
                        println!("  ... and {} more commands", commands.len() - 20);
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Failed to get commands:".red(), e);
                }
            }
        } else {
            eprintln!(
                "{} {}",
                "FFI not available:".yellow(),
                "Initialization failed"
            );
        }
    }
}
