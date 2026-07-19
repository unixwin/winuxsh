// Custom completer for WinSH
// Integrates command, path, and variable completion

use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use reedline::{Completer, Span, Suggestion};
use crate::completion::{CompletionContext, CompletionPlugin, CompletionResult};
use crate::completion::path::PathCompleter;
use crate::completion::variables::VariableCompleter;
use crate::completion::external::{
    CommandCompletionPlugin, CommandDef, ExternalCompletionPlugin,
};

/// State shared with completer
pub struct CompletionState {
    pub current_dir: PathBuf,
    pub env_vars: HashMap<String, String>,
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

    /// Load completion definitions from a list of directories.
    /// Each directory is scanned for `<cmd>.toml` and `<cmd>.bash` files.
    /// Also registers the `CommandCompletionPlugin` if not already present.
    pub fn load_completion_dirs(&mut self, dirs: &[PathBuf]) {
        self.load_completion_dirs_with_definitions(dirs, Vec::new());
    }

    /// Load translated definitions before user directories, so native TOML
    /// files remain the highest-priority override surface.
    pub fn load_completion_dirs_with_definitions(
        &mut self,
        dirs: &[PathBuf],
        definitions: Vec<CommandDef>,
    ) {
        let has_command_plugin = self
            .plugins
            .iter()
            .any(|p| p.name() == "command-completion");
        if !has_command_plugin {
            self.add_plugin(Arc::new(CommandCompletionPlugin));
        }
        let mut external = ExternalCompletionPlugin::new();
        external.load_definitions(definitions);
        for dir in dirs {
            external.load_dir(dir);
        }
        self.add_plugin(Arc::new(external));
    }
}

/// Custom completer for WinSH
pub struct WinuxshCompleter {
    state: Arc<Mutex<CompletionState>>,
}

impl WinuxshCompleter {
    /// Create a new completer with shared state
    pub fn new(state: Arc<Mutex<CompletionState>>) -> Self {
        Self {
            state,
        }
    }

    /// Update state
    pub fn update_state(&self, current_dir: PathBuf, env_vars: HashMap<String, String>) {
        if let Ok(mut state) = self.state.lock() {
            state.current_dir = current_dir;
            state.env_vars = env_vars;
        }
    }

    /// Complete input
    fn complete_input(&mut self, input: &str, cursor_pos: usize) -> Vec<Suggestion> {
        let (current_dir, env_vars, plugins) = if let Ok(state) = self.state.lock() {
            (state.current_dir.clone(), state.env_vars.clone(), state.plugins.clone())
        } else {
            return Vec::new();
        };

        let context = CompletionContext::new(current_dir, input.to_string(), cursor_pos);
        let mut all_suggestions = Vec::new();

        // Try each plugin in order; only the first non-None result is used
        for plugin in &plugins {
            if let Some(result) = plugin.complete(&context) {
                // Found a result, format it
                let formatted = self.format_completions(result, input, cursor_pos);
                all_suggestions.extend(formatted);
            }
        }

        // Fallback to built-in path/variable/command completers
        if all_suggestions.is_empty() {
            if context.is_variable_completion() {
                if let Ok(Some(result)) = VariableCompleter::complete(&context, &env_vars) {
                    return self.format_completions(result, input, cursor_pos);
                }
            }
            if context.is_path_completion() {
                if let Ok(Some(result)) = PathCompleter::complete(&context) {
                    all_suggestions.extend(self.format_completions(result, input, cursor_pos));
                }
            }
        }

        all_suggestions
    }
    fn format_completions(
        &self,
        result: CompletionResult,
        input: &str,
        cursor_pos: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();
        let span_context =
            CompletionContext::new(PathBuf::new(), input.to_string(), cursor_pos);
        let (span_start, span_end) = span_context
            .current_word_span()
            .unwrap_or((cursor_pos, cursor_pos));

        for (i, completion) in result.completions.iter().enumerate() {
            let description = result.descriptions.get(i).and_then(|d| d.as_deref());

            suggestions.push(Suggestion {
                value: completion.clone(),
                description: description.map(|s| s.to_string()),
                style: None,
                extra: None,
                span: Span {
                    start: span_start,
                    end: span_end,
                },
                append_whitespace: true,
            });
        }

        suggestions
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
        let state = Arc::new(Mutex::new(CompletionState::new(PathBuf::from("/home/user"))));
        let completer = WinuxshCompleter::new(state);
        assert!(completer.state.lock().is_ok());
    }

    #[test]
    fn test_load_completion_dirs() {
        let mut state = CompletionState::new(PathBuf::from("."));
        state.load_completion_dirs(&[]);
        // Should have registered at least the command plugin
        assert!(state.plugins.iter().any(|p| p.name() == "command-completion"));
    }

    #[test]
    fn completer_span_covers_escaped_shell_word() {
        let temp_dir = unique_temp_dir("winuxsh-completer-span");
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join("two words.txt"), "two").unwrap();

        let state = Arc::new(Mutex::new(CompletionState::new(temp_dir.clone())));
        let mut completer = WinuxshCompleter::new(state);
        let input = "ls two\\ w";
        let suggestions = completer.complete(input, input.len());

        let suggestion = suggestions
            .iter()
            .find(|suggestion| suggestion.value == "two\\ words.txt")
            .unwrap_or_else(|| panic!("missing suggestion, got {suggestions:?}"));
        assert_eq!(suggestion.span.start, 3);
        assert_eq!(suggestion.span.end, input.len());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
    }
}

