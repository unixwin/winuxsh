// Custom completer for WinSH
// Integrates command, path, and variable completion

use crate::array::ArrayValue;
use crate::completion::path::PathCompleter;
use crate::completion::variables::VariableCompleter;
use crate::completion::{CompletionContext, CompletionPlugin, CompletionResult};
use reedline::{Completer, Span, Suggestion};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// State shared with completer
pub struct CompletionState {
    pub current_dir: PathBuf,
    pub env_vars: HashMap<String, ArrayValue>,
    /// Registered completion plugins (e.g. command completion, external tool completion)
    pub plugins: Vec<Arc<dyn CompletionPlugin>>,
}

impl CompletionState {
    pub fn new(current_dir: PathBuf) -> Self {
        Self {
            current_dir,
            env_vars: HashMap::new(),
            plugins: Vec::new(),
        }
    }

    /// Register a completion plugin
    pub fn add_plugin(&mut self, plugin: Arc<dyn CompletionPlugin>) {
        plugin.on_init();
        self.plugins.push(plugin);
    }
}

/// Custom completer for WinSH
pub struct WinuxshCompleter {
    state: Arc<Mutex<CompletionState>>,
}

impl WinuxshCompleter {
    /// Create a new completer with shared state
    pub fn new(state: Arc<Mutex<CompletionState>>) -> Self {
        Self { state }
    }

    /// Update state
    pub fn update_state(&self, current_dir: PathBuf, env_vars: HashMap<String, ArrayValue>) {
        if let Ok(mut state) = self.state.lock() {
            state.current_dir = current_dir;
            state.env_vars = env_vars;
        }
    }

    /// Complete input
    fn complete_input(&mut self, input: &str, cursor_pos: usize) -> Vec<Suggestion> {
        let (current_dir, env_vars, plugins) = if let Ok(state) = self.state.lock() {
            (
                state.current_dir.clone(),
                state.env_vars.clone(),
                state.plugins.clone(),
            )
        } else {
            return Vec::new();
        };

        let context = CompletionContext::new(current_dir.clone(), input.to_string(), cursor_pos);

        // Built-in: path completion (highest priority for explicit paths)
        // Only short-circuit when the path completer actually returned candidates.
        // An empty result means "no path matches" — fall through to plugins.
        if context.is_path_completion() {
            if let Ok(Some(result)) = PathCompleter::complete(&context) {
                if !result.completions.is_empty() {
                    return self.format_completions(result, input, cursor_pos);
                }
            }
        }

        // Built-in: variable completion
        if context.is_variable_completion() {
            if let Ok(Some(result)) = VariableCompleter::complete(&context, &env_vars) {
                return self.format_completions(result, input, cursor_pos);
            }
        }

        // Plugin chain: command completion and external tool completion
        for plugin in &plugins {
            if let Some(result) = plugin.complete(&context) {
                return self.format_completions(result, input, cursor_pos);
            }
        }

        Vec::new()
    }

    /// Format completions as suggestions
    fn format_completions(
        &self,
        result: CompletionResult,
        input: &str,
        cursor_pos: usize,
    ) -> Vec<Suggestion> {
        let completions = result.completions;
        let result_descriptions = result.descriptions;

        // Calculate span for the word being completed
        // Find the start of the current word
        let word_start = input[..cursor_pos]
            .rfind(|c: char| c.is_whitespace() || c == ';' || c == '|' || c == '&')
            .map(|pos| pos + 1)
            .unwrap_or(0);

        // For path completion, we need to handle path separators correctly
        // Only replace the part after the last path separator
        let before_cursor = &input[..cursor_pos];
        let last_path_sep = before_cursor.rfind(|c: char| c == '/' || c == '\\');

        let span = if let Some(sep_pos) = last_path_sep {
            // For paths, only replace after the last separator
            Span {
                start: sep_pos + 1,
                end: cursor_pos,
            }
        } else {
            // For commands and variables, replace the whole word
            Span {
                start: word_start,
                end: cursor_pos,
            }
        };

        // Pad descriptions so they align at the same column
        let max_value_len = completions.iter().map(|c| c.len()).max().unwrap_or(0);

        completions
            .into_iter()
            .zip(result_descriptions.into_iter())
            .map(|(c, desc)| {
                let padded_desc = desc.map(|d| {
                    let padding = max_value_len.saturating_sub(c.len());
                    format!("{:width$}{}", "", d, width = padding)
                });
                Suggestion {
                    value: c,
                    description: padded_desc,
                    span: span.clone(),
                    ..Default::default()
                }
            })
            .collect()
    }
}

impl Completer for WinuxshCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        self.complete_input(line, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completer_creation() {
        let state = Arc::new(Mutex::new(CompletionState::new(PathBuf::from(
            "/home/user",
        ))));
        let completer = WinuxshCompleter::new(state);
    }
}
