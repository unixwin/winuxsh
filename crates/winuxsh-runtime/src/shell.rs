//! Shell state and execution entry point
//!
//! Wraps a `rubash::Executor` and provides the interactive shell machinery
//! (prompt, history, completion). All shell language semantics are delegated
//! to rubash; this layer only adds the Windows-facing UX.


use std::sync::{Arc, Mutex};

use reedline::Reedline;
use rubash::{executor::Executor, lexer::tokenize, parser::parse};

use crate::completion::CompletionState;
use crate::config::{load as load_config, AutosuggestConfig, EditorMode, SyntaxHighlightConfig};
use crate::prompt::WinuxshPrompt;
use crate::zsh_compat::{
    apply_alias, apply_safe_aliases, apply_safe_env, completion_defs_from_report,
    dynamic_completion_defs_from_report_with_options, git_prompt_format_from_report, scan,
    DynamicCompletionRunOptions, ZshImportOptions,
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

        Ok(Self {
            executor,
            completion_state,
            prompt,
            history_path,
            editor_mode: config.editor.edit_mode,
            autosuggest: config.zsh.autosuggestions.with_env_overrides(),
            syntax_highlighting: syntax_highlighting.with_env_overrides(),
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
