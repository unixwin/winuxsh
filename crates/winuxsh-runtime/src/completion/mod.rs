// Completion module for WinSH
// Provides Tab completion for commands, paths, and variables

pub mod command;
pub mod completer;
pub mod external;
pub mod bash_import;
pub mod path;
pub mod variables;

pub use completer::{WinuxshCompleter, CompletionState};

use std::path::PathBuf;

/// Completion context
pub struct CompletionContext {
    /// Current working directory
    pub current_dir: PathBuf,
    /// Current input line
    pub input: String,
    /// Cursor position in input
    pub cursor_pos: usize,
}

impl CompletionContext {
    pub fn new(current_dir: PathBuf, input: String, cursor_pos: usize) -> Self {
        Self {
            current_dir,
            input,
            cursor_pos,
        }
    }

    /// Get the word under cursor
    pub fn get_current_word(&self) -> Option<String> {
        // Clamp cursor_pos to a valid char boundary
        let pos = self.cursor_pos.min(self.input.len());
        let pos = floor_char_boundary(&self.input, pos);
        let before_cursor = &self.input[..pos];

        // Find the start of the current word
        let word_start = before_cursor
            .rfind(|c: char| c.is_whitespace() || c == ';' || c == '|' || c == '&')
            .map(|p| ceil_char_boundary(before_cursor, p + 1))
            .unwrap_or(0);

        if word_start < before_cursor.len() {
            Some(before_cursor[word_start..].to_string())
        } else {
            None
        }
    }

    /// Check if cursor is at command position (first word or after separator)
    pub fn is_command_position(&self) -> bool {
        let pos = self.cursor_pos.min(self.input.len());
        let pos = floor_char_boundary(&self.input, pos);
        let before_cursor = &self.input[..pos];

        if before_cursor.trim().is_empty() {
            return true;
        }

        let last_sep = before_cursor
            .rfind(|c: char| c == ';' || c == '|' || c == '&' || c == '\n');

        if let Some(p) = last_sep {
            let skip = ceil_char_boundary(before_cursor, p + 1);
            before_cursor[skip..].trim().is_empty()
        } else {
            before_cursor.trim_start().is_empty()
        }
    }

    /// Check if current word is a path (contains / or \ or starts with .)
    pub fn is_path_completion(&self) -> bool {
        if let Some(word) = self.get_current_word() {
            // Flag prefixes are never path completions
            if word.starts_with('-') {
                return false;
            }
            // Explicit path indicators
            if word.contains('/') || word.contains('\\') || word.starts_with('.') {
                return true;
            }
            // If not at command position, treat as path
            if !self.is_command_position() {
                return true;
            }
            false
        } else {
            false
        }
    }

    /// Check if current word is a variable (starts with $)
    pub fn is_variable_completion(&self) -> bool {
        if let Some(word) = self.get_current_word() {
            word.starts_with('$')
        } else {
            false
        }
    }

    /// Get the command name at the start of the current input line segment
    pub fn get_command_name(&self) -> Option<String> {
        let pos = self.cursor_pos.min(self.input.len());
        let pos = floor_char_boundary(&self.input, pos);
        let before_cursor = &self.input[..pos];

        let cmd_start = before_cursor
            .rfind(|c: char| c == ';' || c == '|' || c == '&')
            .map(|p| ceil_char_boundary(before_cursor, p + 1))
            .unwrap_or(0);

        let segment = before_cursor[cmd_start..].trim_start();
        segment.split_whitespace().next().map(|s| s.to_string())
    }

    /// Get the token immediately before the current word (the previous token)
    pub fn get_prev_token(&self) -> Option<String> {
        let pos = self.cursor_pos.min(self.input.len());
        let pos = floor_char_boundary(&self.input, pos);
        let before_cursor = &self.input[..pos];

        // Find the start of the current word
        let word_start = before_cursor
            .rfind(|c: char| c.is_whitespace() || c == ';' || c == '|' || c == '&')
            .map(|p| ceil_char_boundary(before_cursor, p + 1))
            .unwrap_or(0);

        // Everything before the current word
        let before_word = before_cursor[..word_start].trim_end();
        before_word.split_whitespace().last().map(|s| s.to_string())
    }
}

/// Trait for pluggable completion providers.
/// Implementations must be Send + Sync so they can be stored behind Arc<Mutex<>>.
pub trait CompletionPlugin: Send + Sync {
    /// Identifier for this plugin (used in logs and config)
    fn name(&self) -> &str;

    /// Attempt to complete the current context.
    /// Return None to pass through to the next plugin.
    fn complete(&self, context: &CompletionContext) -> Option<CompletionResult>;

    /// Called once when the shell starts up (e.g. warm caches)
    fn on_init(&self) {}

    /// Called after every command execution (e.g. invalidate caches on cd)
    fn on_command_executed(&self, _cmd: &str) {}

    /// Called when the working directory changes
    fn on_directory_changed(&self, _new_dir: &std::path::Path) {}
}

/// Completion result
pub struct CompletionResult {
    /// Completions
    pub completions: Vec<String>,
    /// Per-completion description (parallel to `completions`; None = no description)
    pub descriptions: Vec<Option<String>>,
    /// Common prefix (for partial completion)
    pub common_prefix: Option<String>,
}

impl CompletionResult {
    pub fn new(completions: Vec<String>) -> Self {
        let common_prefix = Self::find_common_prefix(&completions);
        let len = completions.len();
        Self {
            completions,
            descriptions: vec![None; len],
            common_prefix,
        }
    }

    pub fn with_descriptions(completions: Vec<String>, descriptions: Vec<Option<String>>) -> Self {
        let common_prefix = Self::find_common_prefix(&completions);
        Self {
            completions,
            descriptions,
            common_prefix,
        }
    }

    fn find_common_prefix(completions: &[String]) -> Option<String> {
        if completions.is_empty() {
            return None;
        }

        let first = &completions[0];
        let mut prefix_len = first.len();

        for completion in completions.iter().skip(1) {
            while !completion.starts_with(&first[..prefix_len]) && prefix_len > 0 {
                prefix_len -= 1;
            }
            if prefix_len == 0 {
                return None;
            }
        }

        Some(first[..prefix_len].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_word() {
        let ctx = CompletionContext::new(
            PathBuf::from("/home/user"),
            "echo hello world".to_string(),
            10,
        );
        assert_eq!(ctx.get_current_word(), Some("hello".to_string()));
    }

    #[test]
    fn test_is_path_completion() {
        let ctx = CompletionContext::new(
            PathBuf::from("/home/user"),
            "cat /tmp/fil".to_string(),
            12,
        );
        assert!(ctx.is_path_completion());
    }

    #[test]
    fn test_is_variable_completion() {
        let ctx = CompletionContext::new(
            PathBuf::from("/home/user"),
            "echo $HOM".to_string(),
            9,
        );
        assert!(ctx.is_variable_completion());
    }
}

// ─── Char-boundary helpers ────────────────────────────────────────────────────

/// Round `pos` down to the nearest UTF-8 char boundary in `s`.
/// If `pos >= s.len()` returns `s.len()`.
fn floor_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    // Walk backwards until we land on a char boundary
    let mut p = pos;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Round `pos` up to the nearest UTF-8 char boundary in `s`.
/// If `pos >= s.len()` returns `s.len()`.
fn ceil_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos;
    while p < s.len() && !s.is_char_boundary(p) {
        p += 1;
    }
    p
}