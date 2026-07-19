// Path completion for WinSH
// Provides Tab completion for files and directories

use std::cmp::Ordering;
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
        let query = PathQuery::from_word(&context.current_dir, &word);
        let base_dir = Self::normalize_path(&query.base_dir);

        // Check if base directory exists
        if !base_dir.exists() || !base_dir.is_dir() {
            return Ok(None);
        }

        Self::complete_directory(&base_dir, &query, directories_only)
    }

    /// Complete entries in a directory
    fn complete_directory(
        base_dir: &Path,
        query: &PathQuery,
        directories_only: bool,
    ) -> Result<Option<CompletionResult>> {
        let entries = match fs::read_dir(base_dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(None),
        };

        let mut completions: Vec<PathCandidate> = Vec::new();

        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Check if matches prefix (case-insensitive)
            if file_name
                .to_lowercase()
                .starts_with(&query.prefix.to_lowercase())
            {
                let file_type = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                if directories_only && !file_type.is_dir() {
                    continue;
                }
                if file_name.starts_with('.') && !query.prefix.starts_with('.') {
                    continue;
                }

                completions.push(PathCandidate::new(
                    file_type.is_dir(),
                    &query.display_prefix,
                    &file_name,
                ));
            }
        }

        if completions.is_empty() {
            Ok(None)
        } else {
            completions.sort();
            Ok(Some(CompletionResult::new(
                completions
                    .into_iter()
                    .map(|candidate| candidate.completion)
                    .collect(),
            )))
        }
    }

    /// Normalize path for Windows/Unix compatibility
    fn normalize_path(path: &Path) -> PathBuf {
        let path_str = path.to_string_lossy().to_string();

        // Convert Unix paths to Windows paths
        #[cfg(windows)]
        let normalized = path_str.replace('/', "\\");
        #[cfg(not(windows))]
        let normalized = path_str;

        PathBuf::from(normalized)
    }
}

#[derive(Debug)]
struct PathQuery {
    base_dir: PathBuf,
    prefix: String,
    display_prefix: String,
}

impl PathQuery {
    fn from_word(current_dir: &Path, word: &str) -> Self {
        if word.is_empty() {
            return Self {
                base_dir: current_dir.to_path_buf(),
                prefix: String::new(),
                display_prefix: String::new(),
            };
        }

        if word == "." {
            return Self {
                base_dir: current_dir.to_path_buf(),
                prefix: ".".to_string(),
                display_prefix: String::new(),
            };
        }

        if word == "./" || word == ".\\" {
            return Self {
                base_dir: current_dir.to_path_buf(),
                prefix: String::new(),
                display_prefix: word.to_string(),
            };
        }

        if let Some(last_sep) = word.rfind(|c| c == '/' || c == '\\') {
            let display_prefix = word[..=last_sep].to_string();
            let dir_part = &word[..last_sep];
            let prefix = word[last_sep + 1..].to_string();
            let base_dir = if dir_part.is_empty() {
                PathBuf::from(&display_prefix)
            } else {
                resolve_base_dir(current_dir, dir_part)
            };
            return Self {
                base_dir,
                prefix,
                display_prefix,
            };
        }

        Self {
            base_dir: current_dir.to_path_buf(),
            prefix: word.to_string(),
            display_prefix: String::new(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct PathCandidate {
    is_dir: bool,
    sort_key: String,
    completion: String,
}

impl PathCandidate {
    fn new(is_dir: bool, display_prefix: &str, file_name: &str) -> Self {
        let mut completion = format!("{}{}", display_prefix, shell_escape_path(file_name));
        if is_dir {
            completion.push('/');
        }
        Self {
            is_dir,
            sort_key: file_name.to_lowercase(),
            completion,
        }
    }
}

impl Ord for PathCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.is_dir, other.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => self
                .sort_key
                .cmp(&other.sort_key)
                .then_with(|| self.completion.cmp(&other.completion)),
        }
    }
}

impl PartialOrd for PathCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn resolve_base_dir(current_dir: &Path, dir_part: &str) -> PathBuf {
    if is_windows_drive_only(dir_part) {
        return PathBuf::from(format!("{}/", dir_part));
    }

    let path = PathBuf::from(dir_part);
    if path.is_absolute() || has_windows_drive_prefix(dir_part) {
        path
    } else {
        current_dir.join(path)
    }
}

fn shell_escape_path(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if should_escape_path_char(ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn should_escape_path_char(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\t'
            | '\\'
            | '\''
            | '"'
            | '$'
            | '`'
            | '!'
            | '&'
            | ';'
            | '('
            | ')'
            | '<'
            | '>'
            | '|'
            | '*'
            | '?'
            | '['
            | ']'
            | '{'
            | '}'
    )
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_windows_drive_only(value: &str) -> bool {
    value.len() == 2 && has_windows_drive_prefix(value)
}

fn is_directory_only_command(command: &str) -> bool {
    matches!(command, "cd" | "pushd")
}

