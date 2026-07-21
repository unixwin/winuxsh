//! Quote handling for the lexer.

use winsh_core::ShellError;

/// Process ANSI C escape sequences in a dollar-quoted string.
///
/// Supports: \a, \b, \e, \f, \n, \r, \t, \v, \\, \', \", \?, \nnn (octal),
/// \xHH (hex), \uHHHH (unicode), \UHHHHHHHH (unicode)
pub fn process_dollar_quotes(input: &str) -> Result<String, ShellError> {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('a') => result.push('\x07'),
                Some('b') => result.push('\x08'),
                Some('e') => result.push('\x1b'),
                Some('f') => result.push('\x0c'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('v') => result.push('\x0b'),
                Some('\\') => result.push('\\'),
                Some('\'') => result.push('\''),
                Some('"') => result.push('"'),
                Some('?') => result.push('?'),
                Some(d @ '0'..='7') => {
                    // Octal escape: \nnn (up to 3 digits)
                    // The first digit was already consumed by the match
                    let mut octal = String::new();
                    octal.push(d);
                    for _ in 0..2 {
                        if let Some(&c) = chars.peek() {
                            if c >= '0' && c <= '7' {
                                octal.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                    }
                    let value = u32::from_str_radix(&octal, 8).map_err(|_| {
                        ShellError::BadSubstitution(format!("invalid octal: \\{}", octal))
                    })?;
                    if let Some(c) = char::from_u32(value) {
                        result.push(c);
                    } else {
                        return Err(ShellError::BadSubstitution(format!(
                            "invalid char: \\{}",
                            octal
                        )));
                    }
                }
                Some('x') => {
                    // Hex escape: \xHH
                    let hex: String = chars.by_ref().take(2).collect();
                    let value = u32::from_str_radix(&hex, 16).map_err(|_| {
                        ShellError::BadSubstitution(format!("invalid hex: \\x{}", hex))
                    })?;
                    if let Some(c) = char::from_u32(value) {
                        result.push(c);
                    } else {
                        return Err(ShellError::BadSubstitution(format!(
                            "invalid char: \\x{}",
                            hex
                        )));
                    }
                }
                Some('u') => {
                    // Unicode escape: \uHHHH
                    let hex: String = chars.by_ref().take(4).collect();
                    let value = u32::from_str_radix(&hex, 16).map_err(|_| {
                        ShellError::BadSubstitution(format!("invalid unicode: \\u{}", hex))
                    })?;
                    if let Some(c) = char::from_u32(value) {
                        result.push(c);
                    } else {
                        return Err(ShellError::BadSubstitution(format!(
                            "invalid char: \\u{}",
                            hex
                        )));
                    }
                }
                Some('U') => {
                    // Unicode escape: \UHHHHHHHH
                    let hex: String = chars.by_ref().take(8).collect();
                    let value = u32::from_str_radix(&hex, 16).map_err(|_| {
                        ShellError::BadSubstitution(format!("invalid unicode: \\U{}", hex))
                    })?;
                    if let Some(c) = char::from_u32(value) {
                        result.push(c);
                    } else {
                        return Err(ShellError::BadSubstitution(format!(
                            "invalid char: \\U{}",
                            hex
                        )));
                    }
                }
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => {
                    result.push('\\');
                }
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

/// Check if a character is a glob special character.
pub fn is_glob_char(c: char) -> bool {
    c == '*' || c == '?' || c == '[' || c == ']'
}

/// Check if a character needs quoting.
pub fn needs_quoting(c: char) -> bool {
    c.is_whitespace()
        || c == '|'
        || c == '&'
        || c == ';'
        || c == '('
        || c == ')'
        || c == '<'
        || c == '>'
        || c == '"'
        || c == '\''
        || c == '\\'
        || c == '$'
        || c == '`'
        || c == '#'
        || c == '!'
        || is_glob_char(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dollar_quotes_basic() {
        assert_eq!(process_dollar_quotes("hello").unwrap(), "hello");
        assert_eq!(
            process_dollar_quotes("hello\\nworld").unwrap(),
            "hello\nworld"
        );
        assert_eq!(process_dollar_quotes("tab\\there").unwrap(), "tab\there");
    }

    #[test]
    fn test_dollar_quotes_escapes() {
        assert_eq!(process_dollar_quotes("\\\\").unwrap(), "\\");
        assert_eq!(process_dollar_quotes("\\'").unwrap(), "'");
        assert_eq!(process_dollar_quotes("\\\"").unwrap(), "\"");
        assert_eq!(process_dollar_quotes("\\a").unwrap(), "\x07");
        assert_eq!(process_dollar_quotes("\\b").unwrap(), "\x08");
        assert_eq!(process_dollar_quotes("\\e").unwrap(), "\x1b");
        assert_eq!(process_dollar_quotes("\\f").unwrap(), "\x0c");
    }

    #[test]
    fn test_dollar_quotes_hex() {
        assert_eq!(process_dollar_quotes("\\x41").unwrap(), "A");
        assert_eq!(
            process_dollar_quotes("\\x48\\x65\\x6c\\x6c\\x6f").unwrap(),
            "Hello"
        );
    }

    #[test]
    fn test_dollar_quotes_unicode() {
        assert_eq!(process_dollar_quotes("\\u0041").unwrap(), "A");
        assert_eq!(process_dollar_quotes("\\U00000041").unwrap(), "A");
    }

    #[test]
    fn test_dollar_quotes_octal() {
        assert_eq!(process_dollar_quotes("\\101").unwrap(), "A");
        assert_eq!(
            process_dollar_quotes("\\110\\145\\154\\154\\157").unwrap(),
            "Hello"
        );
    }

    #[test]
    fn test_is_glob_char() {
        assert!(is_glob_char('*'));
        assert!(is_glob_char('?'));
        assert!(is_glob_char('['));
        assert!(is_glob_char(']'));
        assert!(!is_glob_char('a'));
        assert!(!is_glob_char(' '));
    }

    #[test]
    fn test_needs_quoting() {
        assert!(needs_quoting(' '));
        assert!(needs_quoting('\t'));
        assert!(needs_quoting('|'));
        assert!(needs_quoting('&'));
        assert!(needs_quoting(';'));
        assert!(!needs_quoting('a'));
        assert!(!needs_quoting('0'));
    }
}
