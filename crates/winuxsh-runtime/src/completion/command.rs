// Command completion for WinSH
// Provides Tab completion for executable commands

use std::env;

use crate::completion::{CompletionContext, CompletionResult};
use anyhow::Result;

/// Command completer
pub struct CommandCompleter;

impl CommandCompleter {
    /// Get all available commands (built-in + PATH)
    pub fn get_all_commands() -> Vec<String> {
        let mut commands = Self::get_builtin_commands();
        commands.extend(Self::get_path_commands());
        commands.sort();
        commands.dedup();
        commands
    }

    /// Get built-in commands
    pub fn get_builtin_commands() -> Vec<String> {
        vec![
            "ls".to_string(),
            "cd".to_string(),
            "pwd".to_string(),
            "echo".to_string(),
            "exit".to_string(),
            "clear".to_string(),
            "cat".to_string(),
            "grep".to_string(),
            "find".to_string(),
            "cp".to_string(),
            "mv".to_string(),
            "rm".to_string(),
            "mkdir".to_string(),
            "jobs".to_string(),
            "fg".to_string(),
            "bg".to_string(),
            "set".to_string(),
            "unset".to_string(),
            "export".to_string(),
            "env".to_string(),
            "help".to_string(),
            "history".to_string(),
            "alias".to_string(),
            "unalias".to_string(),
            "source".to_string(),
            "array".to_string(),
            "plugin".to_string(),
            "theme".to_string(),
            "oh-my-winuxsh".to_string(),
        ]
    }

    /// Get commands from PATH environment variable
    pub fn get_path_commands() -> Vec<String> {
        let mut commands = Vec::new();

        if let Ok(path_env) = env::var("PATH") {
            for path in env::split_paths(&path_env) {
                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries.flatten() {
                        if let Ok(file_type) = entry.file_type() {
                            if file_type.is_file() {
                                let file_name = entry.file_name().to_string_lossy().to_string();
                                
                                // Check if it's executable by extension
                                let is_executable = file_name.ends_with(".exe")
                                    || file_name.ends_with(".bat")
                                    || file_name.ends_with(".cmd")
                                    || file_name.ends_with(".ps1");

                                if is_executable {
                                    // Remove extension for cleaner completion
                                    let name_without_ext = if let Some(pos) = file_name.rfind('.') {
                                        file_name[..pos].to_string()
                                    } else {
                                        file_name.clone()
                                    };
                                    commands.push(name_without_ext);
                                }
                            }
                        }
                    }
                }
            }
        }

        commands
    }

    /// Complete a command name
    pub fn complete(context: &CompletionContext) -> Result<Option<CompletionResult>> {
        let word = match context.get_current_word() {
            Some(w) => w,
            None => return Ok(None),
        };

        // Only complete if we're at a command position
        if !context.is_command_position() {
            return Ok(None);
        }

        // Get all available commands
        let all_commands = Self::get_all_commands();

        // Filter commands that start with the word
        let matches: Vec<String> = all_commands
            .into_iter()
            .filter(|cmd| cmd.to_lowercase().starts_with(&word.to_lowercase()))
            .collect();

        if matches.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResult::new(matches)))
        }
    }

    /// Check if a command exists in PATH
    pub fn command_exists(command: &str) -> bool {
        Self::get_builtin_commands().contains(&command.to_string())
            || Self::get_path_commands().contains(&command.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_commands() {
        let commands = CommandCompleter::get_builtin_commands();
        assert!(commands.contains(&"ls".to_string()));
        assert!(commands.contains(&"cd".to_string()));
    }

    #[test]
    fn test_command_exists() {
        assert!(CommandCompleter::command_exists("ls"));
        assert!(CommandCompleter::command_exists("cd"));

        // Verify PATH command lookup without hardcoding platform-specific binaries.
        if let Some(any_path_cmd) = CommandCompleter::get_path_commands().first() {
            assert!(CommandCompleter::command_exists(any_path_cmd));
        }
    }
}


