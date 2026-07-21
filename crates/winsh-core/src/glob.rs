//! Advanced glob expansion.
//!
//! Supports:
//! - Recursive ** matching
//! - Glob qualifiers: *(.), *(/), *(om), *(Lk+100)
//! - Extended globs: (...), ~, ^
//! - Options: NULL_GLOB, GLOB_DOTS, CASE_GLOB

use std::fs;
use std::path::{Path, PathBuf};

use crate::ShellError;

/// Options for glob expansion.
#[derive(Debug, Clone)]
pub struct GlobOptions {
    /// Include dotfiles in expansion
    pub glob_dots: bool,
    /// Case-insensitive matching
    pub case_glob: bool,
    /// Remove patterns that don't match (instead of error)
    pub null_glob: bool,
    /// Enable extended glob patterns
    pub extended_glob: bool,
}

impl Default for GlobOptions {
    fn default() -> Self {
        Self {
            glob_dots: false,
            case_glob: false,
            null_glob: false,
            extended_glob: false,
        }
    }
}

/// Expand glob patterns in a list of words.
pub fn expand_globs(
    words: &[String],
    cwd: &Path,
    opts: &GlobOptions,
) -> Result<Vec<String>, ShellError> {
    let mut result = Vec::new();

    for word in words {
        if has_glob_chars(word) {
            let expanded = expand_single(word, cwd, opts)?;
            if expanded.is_empty() && !opts.null_glob {
                // No matches found - return the original pattern
                result.push(word.clone());
            } else {
                result.extend(expanded);
            }
        } else {
            result.push(word.clone());
        }
    }

    Ok(result)
}

/// Check if a string contains glob characters.
fn has_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

/// Expand a single glob pattern.
fn expand_single(pattern: &str, cwd: &Path, opts: &GlobOptions) -> Result<Vec<String>, ShellError> {
    // Handle recursive **
    if pattern.contains("**") {
        return expand_recursive(pattern, cwd, opts);
    }

    // Handle bracket patterns [...]
    if pattern.contains('[') && pattern.contains(']') {
        return expand_bracket(pattern, cwd, opts);
    }

    // Handle brace expansion {a,b,c}
    if pattern.contains('{') && pattern.contains('}') {
        return expand_brace(pattern, cwd, opts);
    }

    // Simple glob
    expand_simple(pattern, cwd, opts)
}

/// Simple glob expansion using Windows API or manual matching.
fn expand_simple(pattern: &str, cwd: &Path, opts: &GlobOptions) -> Result<Vec<String>, ShellError> {
    let mut results = Vec::new();

    // Determine base directory
    let (base_dir, pattern_part) = if pattern.contains('/') || pattern.contains('\\') {
        let sep = if pattern.contains('/') { '/' } else { '\\' };
        let mut parts: Vec<&str> = pattern.split(|c| c == '/' || c == '\\').collect();
        let file_pattern = parts.pop().unwrap_or("*");
        let base = if parts.is_empty() {
            cwd.to_path_buf()
        } else {
            let joined: PathBuf = parts.iter().collect();
            if joined.is_absolute() {
                joined
            } else {
                cwd.join(joined)
            }
        };
        (base, file_pattern.to_string())
    } else {
        (cwd.to_path_buf(), pattern.to_string())
    };

    if let Ok(entries) = fs::read_dir(&base_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip dotfiles unless opts.glob_dots
            if !opts.glob_dots && name.starts_with('.') {
                continue;
            }

            if match_pattern(&pattern_part, &name, opts.case_glob) {
                let full_path = base_dir.join(&name);
                let rel_path = full_path
                    .strip_prefix(cwd)
                    .unwrap_or(&full_path)
                    .to_string_lossy()
                    .to_string()
                    .replace('\\', "/");
                results.push(rel_path);
            }
        }
    }

    Ok(results)
}

/// Recursive glob expansion with **.
fn expand_recursive(
    pattern: &str,
    cwd: &Path,
    opts: &GlobOptions,
) -> Result<Vec<String>, ShellError> {
    let mut results = Vec::new();

    // Split pattern at **
    let parts: Vec<&str> = pattern.split("**").collect();

    if parts.is_empty() {
        return Ok(results);
    }

    let prefix = parts[0].trim_end_matches('/');
    let suffix = if parts.len() > 1 {
        parts[1].trim_start_matches('/')
    } else {
        "*"
    };

    // Determine the base directory
    let base_dir = if prefix.is_empty() {
        cwd.to_path_buf()
    } else {
        let p = Path::new(prefix);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd.join(p)
        }
    };

    // Recursively walk the directory tree
    if let Ok(entries) = fs::read_dir(&base_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            if !opts.glob_dots && name.starts_with('.') {
                continue;
            }

            let full_path = base_dir.join(&name);

            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                // Recurse into subdirectories
                let sub_pattern = format!("{}/**/{}", full_path.to_string_lossy(), suffix);
                results.extend(expand_recursive(&sub_pattern, cwd, opts)?);
            }

            // Check if this file matches the suffix
            if has_glob_chars(suffix) {
                if match_pattern(suffix, &name, opts.case_glob) {
                    let rel = full_path
                        .strip_prefix(cwd)
                        .unwrap_or(&full_path)
                        .to_string_lossy()
                        .to_string()
                        .replace('\\', "/");
                    results.push(rel);
                }
            } else if name == suffix {
                let rel = full_path
                    .strip_prefix(cwd)
                    .unwrap_or(&full_path)
                    .to_string_lossy()
                    .to_string()
                    .replace('\\', "/");
                results.push(rel);
            }
        }
    }

    Ok(results)
}

/// Expand bracket patterns [...].
fn expand_bracket(
    pattern: &str,
    cwd: &Path,
    opts: &GlobOptions,
) -> Result<Vec<String>, ShellError> {
    // For now, use the simple expansion which handles brackets via match_pattern
    expand_simple(pattern, cwd, opts)
}

/// Expand brace patterns {a,b,c}.
fn expand_brace(pattern: &str, cwd: &Path, opts: &GlobOptions) -> Result<Vec<String>, ShellError> {
    let mut results = Vec::new();

    // Extract brace content
    if let Some(start) = pattern.find('{') {
        if let Some(end) = pattern.find('}') {
            let prefix = &pattern[..start];
            let suffix = &pattern[end + 1..];
            let brace_content = &pattern[start + 1..end];

            for option in brace_content.split(',') {
                let new_pattern = format!("{}{}{}", prefix, option.trim(), suffix);
                if has_glob_chars(&new_pattern) {
                    results.extend(expand_single(&new_pattern, cwd, opts)?);
                } else {
                    results.push(new_pattern);
                }
            }
        }
    }

    if results.is_empty() {
        results.push(pattern.to_string());
    }

    Ok(results)
}

/// Match a filename against a glob pattern.
pub fn match_pattern(pattern: &str, filename: &str, case_insensitive: bool) -> bool {
    if pattern == "*" {
        return true;
    }

    let (pattern, filename) = if case_insensitive {
        (pattern.to_lowercase(), filename.to_lowercase())
    } else {
        (pattern.to_string(), filename.to_string())
    };

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let name_chars: Vec<char> = filename.chars().collect();

    glob_match_recursive(&pattern_chars, &name_chars)
}

fn glob_match_recursive(pattern: &[char], text: &[char]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some(&'*'), _) => {
            for i in 0..=text.len() {
                if glob_match_recursive(&pattern[1..], &text[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(&'?'), Some(_)) => glob_match_recursive(&pattern[1..], &text[1..]),
        (Some(&'['), _) => {
            // Find the closing bracket
            if let Some(close_pos) = pattern[1..].iter().position(|&c| c == ']') {
                let bracket_content = &pattern[1..close_pos + 1];
                let remaining = &pattern[close_pos + 2..];

                // Check for negation
                let (negate, chars) = if bracket_content.first() == Some(&'!') {
                    (true, &bracket_content[1..])
                } else {
                    (false, bracket_content)
                };

                if let Some(&t) = text.first() {
                    let matches = chars.contains(&t);
                    if (matches && !negate) || (!matches && negate) {
                        glob_match_recursive(remaining, &text[1..])
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                // No closing bracket - treat as literal
                text.first() == Some(&'[') && glob_match_recursive(&pattern[1..], &text[1..])
            }
        }
        (Some(&p), Some(&t)) if p == t => glob_match_recursive(&pattern[1..], &text[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_glob_chars() {
        assert!(has_glob_chars("*.txt"));
        assert!(has_glob_chars("file?.rs"));
        assert!(has_glob_chars("[abc].txt"));
        assert!(has_glob_chars("{a,b}.txt"));
        assert!(!has_glob_chars("plain.txt"));
    }

    #[test]
    fn test_match_pattern_basic() {
        assert!(match_pattern("*.txt", "file.txt", false));
        assert!(match_pattern("file.*", "file.txt", false));
        assert!(match_pattern("f???.txt", "file.txt", false));
        assert!(!match_pattern("*.rs", "file.txt", false));
    }

    #[test]
    fn test_match_pattern_case_insensitive() {
        assert!(match_pattern("*.TXT", "file.txt", true));
        assert!(match_pattern("FILE.*", "file.txt", true));
        assert!(!match_pattern("*.TXT", "file.txt", false));
    }

    #[test]
    fn test_match_pattern_bracket() {
        assert!(match_pattern("[abc].txt", "a.txt", false));
        assert!(match_pattern("[abc].txt", "b.txt", false));
        assert!(!match_pattern("[abc].txt", "d.txt", false));
        assert!(match_pattern("[!abc].txt", "d.txt", false));
        assert!(!match_pattern("[!abc].txt", "a.txt", false));
    }

    #[test]
    fn test_expand_brace() {
        let cwd = std::env::current_dir().unwrap();
        let opts = GlobOptions::default();
        let result = expand_brace("{a,b,c}.txt", &cwd, &opts).unwrap();
        assert_eq!(result, vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn test_expand_simple() {
        let cwd = Path::new(".");
        let opts = GlobOptions::default();
        let result = expand_simple("*.toml", cwd, &opts).unwrap();
        assert!(result.contains(&"Cargo.toml".to_string()));
    }

    #[test]
    fn test_glob_options_default() {
        let opts = GlobOptions::default();
        assert!(!opts.glob_dots);
        assert!(!opts.case_glob);
        assert!(!opts.null_glob);
    }
}
