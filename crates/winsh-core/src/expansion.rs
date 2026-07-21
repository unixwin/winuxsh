//! Variable expansion with modifiers.
//!
//! Supports zsh-style parameter expansion:
//! - ${VAR:-default} - use default if unset or empty
//! - ${VAR:=default} - assign default if unset or empty
//! - ${VAR:+alternate} - use alternate if set and non-empty
//! - ${VAR:?error} - error if unset or empty
//! - ${VAR#pattern} - remove shortest matching prefix
//! - ${VAR##pattern} - remove longest matching prefix
//! - ${VAR%pattern} - remove shortest matching suffix
//! - ${VAR%%pattern} - remove longest matching suffix
//! - ${VAR/old/new} - replace first occurrence
//! - ${VAR//old/new} - replace all occurrences
//! - ${#VAR} - string length

use crate::env::Env;
use crate::ShellError;

/// Expand a variable with modifiers.
pub fn expand_variable(
    name: &str,
    modifier: Option<&str>,
    env: &Env,
) -> Result<String, ShellError> {
    match modifier {
        None => {
            // Simple variable expansion
            Ok(env.get(name).unwrap_or("").to_string())
        }
        Some(modifier) => {
            // Parse the modifier
            if modifier.starts_with('-')
                || modifier.starts_with(':')
                || modifier.starts_with('+')
                || modifier.starts_with('?')
                || modifier.starts_with('=')
            {
                expand_default_modifier(name, modifier, env)
            } else if modifier.starts_with('#') && !modifier.starts_with("##") {
                expand_prefix_removal(name, &modifier[1..], false, env)
            } else if modifier.starts_with("##") {
                expand_prefix_removal(name, &modifier[2..], true, env)
            } else if modifier.starts_with('%') && !modifier.starts_with("%%") {
                expand_suffix_removal(name, &modifier[1..], false, env)
            } else if modifier.starts_with("%%") {
                expand_suffix_removal(name, &modifier[2..], true, env)
            } else if modifier.starts_with('/') {
                expand_substitution(name, modifier, env)
            } else if modifier == "#" {
                // ${#VAR} - string length
                let value = env.get(name).unwrap_or("");
                Ok(value.len().to_string())
            } else {
                Err(ShellError::BadSubstitution(format!(
                    "${{{}{}}}",
                    name, modifier
                )))
            }
        }
    }
}

/// Expand default value modifiers: ${VAR:-default}, ${VAR:=default}, ${VAR:+alternate}, ${VAR:?error}
fn expand_default_modifier(name: &str, modifier: &str, env: &Env) -> Result<String, ShellError> {
    let value = env.get(name);
    let is_set = value.is_some();
    let is_empty = value.map(|s| s.is_empty()).unwrap_or(true);

    // Handle colon variants (check for empty too)
    let (check_empty, op, operand) = if modifier.starts_with(':') {
        let rest = &modifier[1..];
        let op = &rest[..1];
        let operand = &rest[1..];
        (true, op, operand)
    } else {
        let op = &modifier[..1];
        let operand = &modifier[1..];
        (false, op, operand)
    };

    let should_use_default = if check_empty {
        !is_set || is_empty
    } else {
        !is_set
    };

    match op {
        "-" => {
            if should_use_default {
                Ok(operand.to_string())
            } else {
                Ok(value.unwrap().to_string())
            }
        }
        "=" => {
            // Note: We can't modify env here since we only have &Env
            // The caller needs to handle this case
            if should_use_default {
                Ok(operand.to_string())
            } else {
                Ok(value.unwrap().to_string())
            }
        }
        "+" => {
            if should_use_default {
                Ok(String::new())
            } else {
                Ok(operand.to_string())
            }
        }
        "?" => {
            if should_use_default {
                let msg = if operand.is_empty() {
                    format!("{}: parameter null or not set", name)
                } else {
                    operand.to_string()
                };
                Err(ShellError::UnboundVariable(msg))
            } else {
                Ok(value.unwrap().to_string())
            }
        }
        _ => Err(ShellError::BadSubstitution(format!(
            "${{{}{}}}",
            name, modifier
        ))),
    }
}

/// Expand prefix removal: ${VAR#pattern} or ${VAR##pattern}
fn expand_prefix_removal(
    name: &str,
    pattern: &str,
    greedy: bool,
    env: &Env,
) -> Result<String, ShellError> {
    let value = env.get(name).unwrap_or("");

    if greedy {
        // ${VAR##pattern} - remove longest matching prefix
        if let Some(pos) = find_longest_prefix_match(value, pattern) {
            Ok(value[pos..].to_string())
        } else {
            Ok(value.to_string())
        }
    } else {
        // ${VAR#pattern} - remove shortest matching prefix
        if let Some(pos) = find_shortest_prefix_match(value, pattern) {
            Ok(value[pos..].to_string())
        } else {
            Ok(value.to_string())
        }
    }
}

/// Expand suffix removal: ${VAR%pattern} or ${VAR%%pattern}
fn expand_suffix_removal(
    name: &str,
    pattern: &str,
    greedy: bool,
    env: &Env,
) -> Result<String, ShellError> {
    let value = env.get(name).unwrap_or("");

    if greedy {
        // ${VAR%%pattern} - remove longest matching suffix
        if let Some(pos) = find_longest_suffix_match(value, pattern) {
            Ok(value[..pos].to_string())
        } else {
            Ok(value.to_string())
        }
    } else {
        // ${VAR%pattern} - remove shortest matching suffix
        if let Some(pos) = find_shortest_suffix_match(value, pattern) {
            Ok(value[..pos].to_string())
        } else {
            Ok(value.to_string())
        }
    }
}

/// Expand substitution: ${VAR/old/new} or ${VAR//old/new}
fn expand_substitution(name: &str, modifier: &str, env: &Env) -> Result<String, ShellError> {
    let value = env.get(name).unwrap_or("");

    if modifier.starts_with("//") {
        // ${VAR//old/new} - replace all occurrences
        let rest = &modifier[2..];
        if let Some((old, new)) = rest.split_once('/') {
            Ok(value.replace(old, new))
        } else {
            Err(ShellError::BadSubstitution(format!(
                "${{{}/{}}}",
                name, modifier
            )))
        }
    } else {
        // ${VAR/old/new} - replace first occurrence
        let rest = &modifier[1..];
        if let Some((old, new)) = rest.split_once('/') {
            Ok(value.replacen(old, new, 1))
        } else {
            Err(ShellError::BadSubstitution(format!(
                "${{{}/{}}}",
                name, modifier
            )))
        }
    }
}

/// Find the shortest matching prefix.
fn find_shortest_prefix_match(value: &str, pattern: &str) -> Option<usize> {
    // Simple glob matching for prefix
    if pattern.is_empty() {
        return Some(0);
    }

    // Try matching from the start
    for i in 0..=value.len() {
        if glob_match(pattern, &value[..i]) {
            return Some(i);
        }
    }
    None
}

/// Find the longest matching prefix.
fn find_longest_prefix_match(value: &str, pattern: &str) -> Option<usize> {
    if pattern.is_empty() {
        return Some(0);
    }

    // Try matching from the end
    for i in (0..=value.len()).rev() {
        if glob_match(pattern, &value[..i]) {
            return Some(i);
        }
    }
    None
}

/// Find the shortest matching suffix.
fn find_shortest_suffix_match(value: &str, pattern: &str) -> Option<usize> {
    if pattern.is_empty() {
        return Some(value.len());
    }

    // Try matching from the end (shortest suffix means largest i)
    for i in (0..=value.len()).rev() {
        if glob_match(pattern, &value[i..]) {
            return Some(i);
        }
    }
    None
}

/// Find the longest matching suffix.
fn find_longest_suffix_match(value: &str, pattern: &str) -> Option<usize> {
    if pattern.is_empty() {
        return Some(value.len());
    }

    // Try matching from the start (longest suffix means smallest i)
    for i in 0..=value.len() {
        if glob_match(pattern, &value[i..]) {
            return Some(i);
        }
    }
    None
}

/// Simple glob matching (supports * and ?)
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    glob_match_recursive(&pattern_chars, &text_chars)
}

/// Recursive glob matching helper.
fn glob_match_recursive(pattern: &[char], text: &[char]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some(&'*'), _) => {
            // Try matching zero or more characters
            for i in 0..=text.len() {
                if glob_match_recursive(&pattern[1..], &text[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(&'?'), Some(_)) => {
            // Match any single character
            glob_match_recursive(&pattern[1..], &text[1..])
        }
        (Some(&p), Some(&t)) if p == t => {
            // Match exact character
            glob_match_recursive(&pattern[1..], &text[1..])
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_simple_variable() {
        let mut env = Env::new();
        env.set("HOME", "/home/user");
        assert_eq!(expand_variable("HOME", None, &env).unwrap(), "/home/user");
    }

    #[test]
    fn test_expand_unset_variable() {
        let env = Env::new();
        assert_eq!(expand_variable("UNSET", None, &env).unwrap(), "");
    }

    #[test]
    fn test_expand_default_value() {
        let mut env = Env::new();
        env.set("VAR", "value");
        assert_eq!(
            expand_variable("VAR", Some("-default"), &env).unwrap(),
            "value"
        );

        let env = Env::new();
        assert_eq!(
            expand_variable("VAR", Some("-default"), &env).unwrap(),
            "default"
        );
    }

    #[test]
    fn test_expand_default_assign() {
        let env = Env::new();
        // Note: With &Env, we can't actually assign, so this just returns the default
        assert_eq!(
            expand_variable("VAR", Some("=default"), &env).unwrap(),
            "default"
        );
    }

    #[test]
    fn test_expand_alternate() {
        let mut env = Env::new();
        env.set("VAR", "value");
        assert_eq!(
            expand_variable("VAR", Some("+alternate"), &env).unwrap(),
            "alternate"
        );

        let env = Env::new();
        assert_eq!(
            expand_variable("VAR", Some("+alternate"), &env).unwrap(),
            ""
        );
    }

    #[test]
    fn test_expand_error() {
        let env = Env::new();
        assert!(expand_variable("VAR", Some("?error message"), &env).is_err());
    }

    #[test]
    fn test_expand_prefix_removal() {
        let mut env = Env::new();
        env.set("FILE", "/path/to/file.tar.gz");

        // ${FILE#*/} - remove shortest prefix up to /
        assert_eq!(
            expand_variable("FILE", Some("#*/"), &env).unwrap(),
            "path/to/file.tar.gz"
        );

        // ${FILE##*/} - remove longest prefix up to /
        assert_eq!(
            expand_variable("FILE", Some("##*/"), &env).unwrap(),
            "file.tar.gz"
        );
    }

    #[test]
    fn test_expand_suffix_removal() {
        let mut env = Env::new();
        env.set("FILE", "/path/to/file.tar.gz");

        // ${FILE%.*} - remove shortest suffix starting with .
        assert_eq!(
            expand_variable("FILE", Some("%.*"), &env).unwrap(),
            "/path/to/file.tar"
        );

        // ${FILE%%.*} - remove longest suffix starting with .
        assert_eq!(
            expand_variable("FILE", Some("%%.*"), &env).unwrap(),
            "/path/to/file"
        );
    }

    #[test]
    fn test_expand_substitution() {
        let mut env = Env::new();
        env.set("VAR", "hello world hello");

        // ${VAR/old/new} - replace first occurrence
        assert_eq!(
            expand_variable("VAR", Some("/hello/bye"), &env).unwrap(),
            "bye world hello"
        );

        // ${VAR//old/new} - replace all occurrences
        assert_eq!(
            expand_variable("VAR", Some("//hello/bye"), &env).unwrap(),
            "bye world bye"
        );
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*.txt", "file.txt"));
        assert!(glob_match("file.*", "file.txt"));
        assert!(glob_match("f*.txt", "file.txt"));
        assert!(!glob_match("*.txt", "file.log"));
        assert!(glob_match("???", "abc"));
        assert!(!glob_match("???", "ab"));
    }
}
