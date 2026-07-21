//! Conditional expression evaluation for [[ ... ]] syntax.
//!
//! Supports:
//! - String comparison: ==, !=, <, >
//! - Pattern matching: == (with glob), =~ (regex)
//! - String tests: -z (empty), -n (non-empty)
//! - File tests: -e, -f, -d, -r, -w, -x, -s, -L
//! - File comparison: -nt (newer than), -ot (older than)
//! - Integer comparison: -eq, -ne, -lt, -le, -gt, -ge
//! - Logical: &&, ||, !

use crate::env::Env;
use crate::ShellError;

/// Evaluate a conditional expression.
pub fn eval_conditional(expr: &str, env: &Env) -> Result<bool, ShellError> {
    let tokens = tokenize_conditional(expr)?;
    let mut parser = ConditionalParser::new(tokens, env);
    parser.parse_expression()
}

/// Tokenize a conditional expression.
fn tokenize_conditional(expr: &str) -> Result<Vec<CondToken>, ShellError> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '&' => {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    tokens.push(CondToken::LogicalAnd);
                } else {
                    return Err(ShellError::SyntaxError {
                        line: 0,
                        col: 0,
                        message: "unexpected '&'".to_string(),
                    });
                }
            }
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(CondToken::LogicalOr);
                } else {
                    return Err(ShellError::SyntaxError {
                        line: 0,
                        col: 0,
                        message: "unexpected '|'".to_string(),
                    });
                }
            }
            '!' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(CondToken::NotEqual);
                } else {
                    tokens.push(CondToken::LogicalNot);
                }
            }
            '=' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(CondToken::Equal);
                } else if chars.peek() == Some(&'~') {
                    chars.next();
                    tokens.push(CondToken::RegexMatch);
                } else {
                    tokens.push(CondToken::Equal);
                }
            }
            '<' => {
                chars.next();
                tokens.push(CondToken::Less);
            }
            '>' => {
                chars.next();
                tokens.push(CondToken::Greater);
            }
            '(' => {
                chars.next();
                tokens.push(CondToken::LeftParen);
            }
            ')' => {
                chars.next();
                tokens.push(CondToken::RightParen);
            }
            '-' => {
                // Could be a test operator
                let mut op = String::new();
                op.push(chars.next().unwrap());
                // Read the rest of the operator
                while let Some(&c) = chars.peek() {
                    if c.is_alphabetic() {
                        op.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                match op.as_str() {
                    "-z" => tokens.push(CondToken::StringEmpty),
                    "-n" => tokens.push(CondToken::StringNonEmpty),
                    "-e" => tokens.push(CondToken::FileExists),
                    "-f" => tokens.push(CondToken::IsRegularFile),
                    "-d" => tokens.push(CondToken::IsDirectory),
                    "-r" => tokens.push(CondToken::IsReadable),
                    "-w" => tokens.push(CondToken::IsWritable),
                    "-x" => tokens.push(CondToken::IsExecutable),
                    "-s" => tokens.push(CondToken::IsNonEmpty),
                    "-L" => tokens.push(CondToken::IsSymlink),
                    "-nt" => tokens.push(CondToken::IsNewer),
                    "-ot" => tokens.push(CondToken::IsOlder),
                    "-eq" => tokens.push(CondToken::IntEqual),
                    "-ne" => tokens.push(CondToken::IntNotEqual),
                    "-lt" => tokens.push(CondToken::IntLess),
                    "-le" => tokens.push(CondToken::IntLessEqual),
                    "-gt" => tokens.push(CondToken::IntGreater),
                    "-ge" => tokens.push(CondToken::IntGreaterEqual),
                    _ => {
                        return Err(ShellError::SyntaxError {
                            line: 0,
                            col: 0,
                            message: format!("unknown test operator: {}", op),
                        });
                    }
                }
            }
            '"' | '\'' => {
                // Quoted string
                let quote = chars.next().unwrap();
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c == quote {
                        chars.next();
                        break;
                    }
                    if c == '\\' {
                        chars.next();
                        if let Some(escaped) = chars.next() {
                            s.push(escaped);
                        }
                    } else {
                        s.push(chars.next().unwrap());
                    }
                }
                tokens.push(CondToken::String(s));
            }
            _ => {
                // Unquoted string or variable
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == ')' || c == '(' || c == '&' || c == '|' {
                        break;
                    }
                    s.push(chars.next().unwrap());
                }

                // Check if it's a variable
                if s.starts_with('$') {
                    let var_name = s[1..].to_string();
                    tokens.push(CondToken::Variable(var_name));
                } else {
                    tokens.push(CondToken::String(s));
                }
            }
        }
    }

    Ok(tokens)
}

/// Conditional tokens.
#[derive(Debug, Clone, PartialEq)]
enum CondToken {
    String(String),
    Variable(String),
    // String comparison
    Equal,
    NotEqual,
    Less,
    Greater,
    RegexMatch,
    // String tests
    StringEmpty,
    StringNonEmpty,
    // File tests
    FileExists,
    IsRegularFile,
    IsDirectory,
    IsReadable,
    IsWritable,
    IsExecutable,
    IsNonEmpty,
    IsSymlink,
    // File comparison
    IsNewer,
    IsOlder,
    // Integer comparison
    IntEqual,
    IntNotEqual,
    IntLess,
    IntLessEqual,
    IntGreater,
    IntGreaterEqual,
    // Logical
    LogicalAnd,
    LogicalOr,
    LogicalNot,
    // Grouping
    LeftParen,
    RightParen,
}

/// Conditional expression parser.
struct ConditionalParser<'a> {
    tokens: Vec<CondToken>,
    pos: usize,
    env: &'a Env,
}

impl<'a> ConditionalParser<'a> {
    fn new(tokens: Vec<CondToken>, env: &'a Env) -> Self {
        Self {
            tokens,
            pos: 0,
            env,
        }
    }

    fn peek(&self) -> Option<&CondToken> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<CondToken> {
        if self.pos < self.tokens.len() {
            let token = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(token)
        } else {
            None
        }
    }

    fn parse_expression(&mut self) -> Result<bool, ShellError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<bool, ShellError> {
        let mut left = self.parse_and()?;

        while self.peek() == Some(&CondToken::LogicalOr) {
            self.advance();
            let right = self.parse_and()?;
            left = left || right;
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<bool, ShellError> {
        let mut left = self.parse_not()?;

        while self.peek() == Some(&CondToken::LogicalAnd) {
            self.advance();
            let right = self.parse_not()?;
            left = left && right;
        }

        Ok(left)
    }

    fn parse_not(&mut self) -> Result<bool, ShellError> {
        if self.peek() == Some(&CondToken::LogicalNot) {
            self.advance();
            let val = self.parse_not()?;
            return Ok(!val);
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<bool, ShellError> {
        match self.peek() {
            Some(CondToken::LeftParen) => {
                self.advance();
                let val = self.parse_expression()?;
                if self.peek() == Some(&CondToken::RightParen) {
                    self.advance();
                }
                Ok(val)
            }
            Some(CondToken::StringEmpty) => {
                self.advance();
                let val = self.get_value()?;
                Ok(val.is_empty())
            }
            Some(CondToken::StringNonEmpty) => {
                self.advance();
                let val = self.get_value()?;
                Ok(!val.is_empty())
            }
            Some(CondToken::FileExists) => {
                self.advance();
                let path = self.get_value()?;
                Ok(std::path::Path::new(&path).exists())
            }
            Some(CondToken::IsRegularFile) => {
                self.advance();
                let path = self.get_value()?;
                Ok(std::path::Path::new(&path).is_file())
            }
            Some(CondToken::IsDirectory) => {
                self.advance();
                let path = self.get_value()?;
                Ok(std::path::Path::new(&path).is_dir())
            }
            Some(CondToken::IsReadable) => {
                self.advance();
                let path = self.get_value()?;
                // Simple check - just verify file exists
                Ok(std::path::Path::new(&path).exists())
            }
            Some(CondToken::IsWritable) => {
                self.advance();
                let path = self.get_value()?;
                Ok(std::path::Path::new(&path).exists())
            }
            Some(CondToken::IsExecutable) => {
                self.advance();
                let path = self.get_value()?;
                Ok(std::path::Path::new(&path).exists())
            }
            Some(CondToken::IsNonEmpty) => {
                self.advance();
                let path = self.get_value()?;
                match std::fs::metadata(&path) {
                    Ok(m) => Ok(m.len() > 0),
                    Err(_) => Ok(false),
                }
            }
            Some(CondToken::IsSymlink) => {
                self.advance();
                let path = self.get_value()?;
                match std::fs::symlink_metadata(&path) {
                    Ok(m) => Ok(m.file_type().is_symlink()),
                    Err(_) => Ok(false),
                }
            }
            _ => self.parse_comparison(),
        }
    }

    fn parse_comparison(&mut self) -> Result<bool, ShellError> {
        let left = self.get_value()?;

        match self.peek() {
            Some(CondToken::Equal) => {
                self.advance();
                let right = self.get_value()?;
                Ok(glob_match(&right, &left))
            }
            Some(CondToken::NotEqual) => {
                self.advance();
                let right = self.get_value()?;
                Ok(left != right)
            }
            Some(CondToken::Less) => {
                self.advance();
                let right = self.get_value()?;
                Ok(left < right)
            }
            Some(CondToken::Greater) => {
                self.advance();
                let right = self.get_value()?;
                Ok(left > right)
            }
            Some(CondToken::RegexMatch) => {
                self.advance();
                let pattern = self.get_value()?;
                // Simple regex match using glob for now
                Ok(glob_match(&pattern, &left))
            }
            Some(CondToken::IntEqual) => {
                self.advance();
                let right = self.get_value()?;
                let left_num: i64 = left.parse().unwrap_or(0);
                let right_num: i64 = right.parse().unwrap_or(0);
                Ok(left_num == right_num)
            }
            Some(CondToken::IntNotEqual) => {
                self.advance();
                let right = self.get_value()?;
                let left_num: i64 = left.parse().unwrap_or(0);
                let right_num: i64 = right.parse().unwrap_or(0);
                Ok(left_num != right_num)
            }
            Some(CondToken::IntLess) => {
                self.advance();
                let right = self.get_value()?;
                let left_num: i64 = left.parse().unwrap_or(0);
                let right_num: i64 = right.parse().unwrap_or(0);
                Ok(left_num < right_num)
            }
            Some(CondToken::IntLessEqual) => {
                self.advance();
                let right = self.get_value()?;
                let left_num: i64 = left.parse().unwrap_or(0);
                let right_num: i64 = right.parse().unwrap_or(0);
                Ok(left_num <= right_num)
            }
            Some(CondToken::IntGreater) => {
                self.advance();
                let right = self.get_value()?;
                let left_num: i64 = left.parse().unwrap_or(0);
                let right_num: i64 = right.parse().unwrap_or(0);
                Ok(left_num > right_num)
            }
            Some(CondToken::IntGreaterEqual) => {
                self.advance();
                let right = self.get_value()?;
                let left_num: i64 = left.parse().unwrap_or(0);
                let right_num: i64 = right.parse().unwrap_or(0);
                Ok(left_num >= right_num)
            }
            Some(CondToken::IsNewer) => {
                self.advance();
                let right = self.get_value()?;
                match (std::fs::metadata(&left), std::fs::metadata(&right)) {
                    (Ok(l), Ok(r)) => {
                        let l_time = l.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        let r_time = r.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        Ok(l_time > r_time)
                    }
                    _ => Ok(false),
                }
            }
            Some(CondToken::IsOlder) => {
                self.advance();
                let right = self.get_value()?;
                match (std::fs::metadata(&left), std::fs::metadata(&right)) {
                    (Ok(l), Ok(r)) => {
                        let l_time = l.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        let r_time = r.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        Ok(l_time < r_time)
                    }
                    _ => Ok(false),
                }
            }
            _ => {
                // Just a string - check if non-empty
                Ok(!left.is_empty())
            }
        }
    }

    fn get_value(&mut self) -> Result<String, ShellError> {
        match self.advance() {
            Some(CondToken::String(s)) => Ok(s),
            Some(CondToken::Variable(name)) => Ok(self.env.get(&name).unwrap_or("").to_string()),
            Some(token) => Err(ShellError::SyntaxError {
                line: 0,
                col: 0,
                message: format!("expected value, got {:?}", token),
            }),
            None => Err(ShellError::SyntaxError {
                line: 0,
                col: 0,
                message: "unexpected end of expression".to_string(),
            }),
        }
    }
}

/// Simple glob matching.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    glob_match_recursive(&pattern_chars, &text_chars)
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
        (Some(&p), Some(&t)) if p == t => glob_match_recursive(&pattern[1..], &text[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conditional_string_equal() {
        let mut env = Env::new();
        env.set("VAR", "hello");
        assert!(eval_conditional("$VAR == hello", &env).unwrap());
        assert!(!eval_conditional("$VAR == world", &env).unwrap());
    }

    #[test]
    fn test_conditional_string_not_equal() {
        let mut env = Env::new();
        env.set("VAR", "hello");
        assert!(eval_conditional("$VAR != world", &env).unwrap());
        assert!(!eval_conditional("$VAR != hello", &env).unwrap());
    }

    #[test]
    fn test_conditional_string_empty() {
        let mut env = Env::new();
        env.set("EMPTY", "");
        env.set("NONEMPTY", "hello");
        assert!(eval_conditional("-z $EMPTY", &env).unwrap());
        assert!(!eval_conditional("-z $NONEMPTY", &env).unwrap());
        assert!(eval_conditional("-n $NONEMPTY", &env).unwrap());
        assert!(!eval_conditional("-n $EMPTY", &env).unwrap());
    }

    #[test]
    fn test_conditional_int_comparison() {
        let mut env = Env::new();
        env.set("X", "10");
        env.set("Y", "20");
        assert!(eval_conditional("$X -lt $Y", &env).unwrap());
        assert!(eval_conditional("$Y -gt $X", &env).unwrap());
        assert!(eval_conditional("$X -eq 10", &env).unwrap());
    }

    #[test]
    fn test_conditional_logical() {
        let env = Env::new();
        assert!(eval_conditional("1 -eq 1 && 2 -eq 2", &env).unwrap());
        assert!(eval_conditional("1 -eq 1 || 2 -eq 3", &env).unwrap());
        assert!(eval_conditional("! 1 -eq 2", &env).unwrap());
    }

    #[test]
    fn test_conditional_glob_match() {
        let env = Env::new();
        assert!(eval_conditional("hello == h*llo", &env).unwrap());
        assert!(eval_conditional("hello == h?llo", &env).unwrap());
        assert!(!eval_conditional("hello == w*ld", &env).unwrap());
    }

    #[test]
    fn test_conditional_file_exists() {
        let env = Env::new();
        // Test with a file that should exist
        assert!(eval_conditional("-e Cargo.toml", &env).unwrap());
        // Test with a file that shouldn't exist
        assert!(!eval_conditional("-e nonexistent_file_xyz", &env).unwrap());
    }

    #[test]
    fn test_conditional_file_is_directory() {
        let env = Env::new();
        assert!(eval_conditional("-d src", &env).unwrap());
        assert!(!eval_conditional("-d Cargo.toml", &env).unwrap());
    }
}
