// Path completion for WinSH
// Provides Tab completion for files and directories

use std::fs;
use std::path::{Path, PathBuf};

use crate::completion::{CompletionContext, CompletionResult};
use anyhow::Result;

/// Path completer
pub struct PathCompleter;

impl PathCompleter {
    /// Complete a path
    pub fn complete(context: &CompletionContext) -> Result<Option<CompletionResult>> {
        let word = context.get_current_word().unwrap_or_default();
        let directories_only = context
            .get_command_name()
            .as_deref()
            .is_some_and(is_directory_only_command);

        // Handle empty word (just complete current directory)
        if word.is_empty() {
            return Self::complete_directory(&context.current_dir, "", false, directories_only);
        }

        // Determine base directory and prefix
        let (base_dir, prefix, add_trailing_slash) =
            if word.starts_with('/') || word.starts_with('\\') {
            // Absolute path
            let word_clone = word.clone();
            let path = if word.starts_with('\\') {
                // Windows UNC path
                PathBuf::from(&word_clone)
            } else {
                // Unix-style absolute path (convert to Windows)
                PathBuf::from(&word_clone)
            };
            
            if let Some(parent) = path.parent() {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                (parent.to_path_buf(), file_name.to_string(), false)
            } else {
                (PathBuf::from("/"), word[1..].to_string(), false)
            }
        } else if word.ends_with('/') || word.ends_with('\\') {
            // Path ends with separator - complete directory contents
            let dir_part = &word[..word.len().saturating_sub(1)];
            let base_dir = context.current_dir.join(dir_part);
            (base_dir, String::new(), false)
        } else if word.contains('/') || word.contains('\\') {
            // Relative path with directory separator
            let last_sep = word.rfind(|c: char| c == '/' || c == '\\').unwrap();
            let dir_part = &word[..last_sep];
            let prefix_part = &word[last_sep + 1..];
            (context.current_dir.join(dir_part), prefix_part.to_string(), false)
        } else if word.starts_with('.') {
            // Current directory reference
            if word == "." || word == "./" {
                (context.current_dir.clone(), String::new(), false)
            } else if word.starts_with("./") {
                (context.current_dir.clone(), word[2..].to_string(), false)
            } else {
                (context.current_dir.clone(), word[1..].to_string(), false)
            }
        } else {
            // No path separator, assume current directory
            (context.current_dir.clone(), word.clone(), false)
        };

        // Normalize base path
        let base_dir = Self::normalize_path(&base_dir);

        // Check if base directory exists
        if !base_dir.exists() || !base_dir.is_dir() {
            return Ok(None);
        }

        Self::complete_directory(&base_dir, &prefix, add_trailing_slash, directories_only)
    }

    /// Complete entries in a directory
    fn complete_directory(
        base_dir: &Path,
        prefix: &str,
        _add_trailing_slash: bool,
        directories_only: bool,
    ) -> Result<Option<CompletionResult>> {
        let entries = match fs::read_dir(base_dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(None),
        };

        let mut completions: Vec<String> = Vec::new();

        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Check if matches prefix (case-insensitive)
            if file_name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                let file_type = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                if directories_only && !file_type.is_dir() {
                    continue;
                }

                // Add separator for directories
                let completion = if file_type.is_dir() {
                    format!("{}/", file_name)
                } else {
                    file_name.clone()
                };

                completions.push(completion);
            }
        }

        if completions.is_empty() {
            Ok(None)
        } else {
            completions.sort();
            Ok(Some(CompletionResult::new(completions)))
        }
    }

    /// Normalize path for Windows/Unix compatibility
    fn normalize_path(path: &Path) -> PathBuf {
        let path_str = path.to_string_lossy().to_string();

        // Convert Unix paths to Windows paths
        let normalized = path_str.replace('/', "\\");
        PathBuf::from(normalized)
    }
}

fn is_directory_only_command(command: &str) -> bool {
    matches!(command, "cd" | "pushd")
}

