//! Shell state and execution entry point
//!
//! Wraps a `rubash::Executor` and provides the interactive shell machinery
//! (prompt, history, completion). All shell language semantics are delegated
//! to rubash; this layer only adds the Windows-facing UX.


use std::sync::{Arc, Mutex};

use reedline::Reedline;
use rubash::{executor::Executor, lexer::tokenize, parser::parse};

use crate::completion::CompletionState;
use crate::config::{load as load_config, EditorMode};
use crate::prompt::WinuxshPrompt;

use crate::winuxcmd;

/// Top-level shell state.
pub struct Shell {
    pub executor: Executor,
    pub completion_state: Arc<Mutex<CompletionState>>,
    pub prompt: WinuxshPrompt,
    pub history_path: std::path::PathBuf,
    pub editor_mode: EditorMode,
    pub line_editor: Option<Reedline>,
}

impl Shell {
    /// Construct a fresh shell: load config, install Ctrl+C handler, inject
    /// winuxcmd onto PATH, set up completion state and history.
    pub fn new() -> anyhow::Result<Self> {
        // 1. WinuxCmd PATH injection (best-effort).
        if let Err(e) = winuxcmd::ensure_on_path() {
            log::warn!("winuxcmd not on PATH: {}", e);
        }

        // 2. Load config from ~/.winshrc.toml.
        let config = load_config();

        // 3. Build rubash Executor.
        let mut executor = Executor::new();

        // 4. Wire aliases from config into rubash.
        for (name, value) in &config.aliases {
            executor.set_env(&format!("BASH_ALIASES[{}]", name), value);
        }

        // 5. Prompt + theme.
        let prompt = WinuxshPrompt::new(config.shell.prompt_format.clone(), &config.theme_name);

        // 6. History file in home dir.
        let history_path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".winuxsh_history");

        // 7. Completion state.
        let completion_state = Arc::new(Mutex::new(CompletionState::new(
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        )));

        // 8. Load completion dirs from config (inline, not in thread).
        {
            let mut s = completion_state.lock().unwrap();
            s.load_completion_dirs(&config.completion_dirs);
        }

        Ok(Self {
            executor,
            completion_state,
            prompt,
            history_path,
            editor_mode: config.editor.edit_mode,
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
