//! Arithmetic expression evaluation.
//!
//! Supports:
//! - Basic arithmetic: +, -, *, /, %
//! - Bitwise: &, |, ^, ~, <<, >>
//! - Comparison: ==, !=, <, <=, >, >=
//! - Logical: &&, ||, !
//! - Assignment: =, +=, -=, *=, /=, %=
//! - Increment/Decrement: ++, --
//! - Ternary: ? :
//! - Parentheses for grouping

use crate::env::Env;
use crate::ShellError;

/// Evaluate an arithmetic expression.
pub fn eval_arithmetic(expr: &str, env: &Env) -> Result<i64, ShellError> {
    let tokens = tokenize_arithmetic(expr)?;
    let mut parser = ArithmeticParser::new(tokens, env);
    parser.parse_expression()
}

/// Tokenize an arithmetic expression.
fn tokenize_arithmetic(expr: &str) -> Result<Vec<ArithToken>, ShellError> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '+' => {
                chars.next();
                if chars.peek() == Some(&'+') {
                    chars.next();
                    tokens.push(ArithToken::Increment);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::AddAssign);
                } else {
                    tokens.push(ArithToken::Plus);
                }
            }
            '-' => {
                chars.next();
                if chars.peek() == Some(&'-') {
                    chars.next();
                    tokens.push(ArithToken::Decrement);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::SubAssign);
                } else {
                    tokens.push(ArithToken::Minus);
                }
            }
            '*' => {
                chars.next();
                if chars.peek() == Some(&'*') {
                    chars.next();
                    tokens.push(ArithToken::Power);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::MulAssign);
                } else {
                    tokens.push(ArithToken::Multiply);
                }
            }
            '/' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::DivAssign);
                } else {
                    tokens.push(ArithToken::Divide);
                }
            }
            '%' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::ModAssign);
                } else {
                    tokens.push(ArithToken::Modulo);
                }
            }
            '&' => {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    tokens.push(ArithToken::LogicalAnd);
                } else {
                    tokens.push(ArithToken::BitwiseAnd);
                }
            }
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(ArithToken::LogicalOr);
                } else {
                    tokens.push(ArithToken::BitwiseOr);
                }
            }
            '^' => {
                chars.next();
                tokens.push(ArithToken::BitwiseXor);
            }
            '~' => {
                chars.next();
                tokens.push(ArithToken::BitwiseNot);
            }
            '!' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::NotEqual);
                } else {
                    tokens.push(ArithToken::LogicalNot);
                }
            }
            '=' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::Equal);
                } else {
                    tokens.push(ArithToken::Assign);
                }
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'<') {
                    chars.next();
                    tokens.push(ArithToken::LeftShift);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::LessEqual);
                } else {
                    tokens.push(ArithToken::Less);
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(ArithToken::RightShift);
                } else if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(ArithToken::GreaterEqual);
                } else {
                    tokens.push(ArithToken::Greater);
                }
            }
            '?' => {
                chars.next();
                tokens.push(ArithToken::Question);
            }
            ':' => {
                chars.next();
                tokens.push(ArithToken::Colon);
            }
            '(' => {
                chars.next();
                tokens.push(ArithToken::LeftParen);
            }
            ')' => {
                chars.next();
                tokens.push(ArithToken::RightParen);
            }
            '0'..='9' => {
                let mut num = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit()
                        || c == 'x'
                        || c == 'X'
                        || c == 'o'
                        || c == 'O'
                        || c == 'b'
                        || c == 'B'
                        || (c.is_ascii_hexdigit() && num.starts_with("0x"))
                    {
                        num.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                let value = parse_number(&num)?;
                tokens.push(ArithToken::Number(value));
            }
            '$' => {
                chars.next();
                if chars.peek() == Some(&'{') {
                    // ${VAR}
                    chars.next();
                    let mut var_name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next();
                            break;
                        }
                        var_name.push(chars.next().unwrap());
                    }
                    tokens.push(ArithToken::Variable(var_name));
                } else {
                    // $VAR
                    let mut var_name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            var_name.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    tokens.push(ArithToken::Variable(var_name));
                }
            }
            _ if c.is_alphabetic() || c == '_' => {
                // Variable name
                let mut var_name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        var_name.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                tokens.push(ArithToken::Variable(var_name));
            }
            _ => {
                return Err(ShellError::ArithmeticSyntax(format!(
                    "unexpected character: '{}'",
                    c
                )));
            }
        }
    }

    Ok(tokens)
}

/// Parse a number literal.
fn parse_number(s: &str) -> Result<i64, ShellError> {
    if s.starts_with("0x") || s.starts_with("0X") {
        i64::from_str_radix(&s[2..], 16)
            .map_err(|_| ShellError::ArithmeticSyntax(format!("invalid hex number: {}", s)))
    } else if s.starts_with("0o") || s.starts_with("0O") {
        i64::from_str_radix(&s[2..], 8)
            .map_err(|_| ShellError::ArithmeticSyntax(format!("invalid octal number: {}", s)))
    } else if s.starts_with("0b") || s.starts_with("0B") {
        i64::from_str_radix(&s[2..], 2)
            .map_err(|_| ShellError::ArithmeticSyntax(format!("invalid binary number: {}", s)))
    } else {
        s.parse::<i64>()
            .map_err(|_| ShellError::ArithmeticSyntax(format!("invalid number: {}", s)))
    }
}

/// Arithmetic tokens.
#[derive(Debug, Clone, PartialEq)]
enum ArithToken {
    Number(i64),
    Variable(String),
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Power,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitwiseNot,
    LeftShift,
    RightShift,
    LogicalAnd,
    LogicalOr,
    LogicalNot,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    Increment,
    Decrement,
    Question,
    Colon,
    LeftParen,
    RightParen,
}

/// Arithmetic expression parser.
struct ArithmeticParser<'a> {
    tokens: Vec<ArithToken>,
    pos: usize,
    env: &'a Env,
}

impl<'a> ArithmeticParser<'a> {
    fn new(tokens: Vec<ArithToken>, env: &'a Env) -> Self {
        Self {
            tokens,
            pos: 0,
            env,
        }
    }

    fn peek(&self) -> Option<&ArithToken> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<ArithToken> {
        if self.pos < self.tokens.len() {
            let token = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(token)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: &ArithToken) -> Result<(), ShellError> {
        match self.advance() {
            Some(token) if token == *expected => Ok(()),
            Some(token) => Err(ShellError::ArithmeticSyntax(format!(
                "expected {:?}, got {:?}",
                expected, token
            ))),
            None => Err(ShellError::ArithmeticSyntax(
                "unexpected end of expression".to_string(),
            )),
        }
    }

    fn parse_expression(&mut self) -> Result<i64, ShellError> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<i64, ShellError> {
        let cond = self.parse_logical_or()?;

        if self.peek() == Some(&ArithToken::Question) {
            self.advance();
            let true_val = self.parse_expression()?;
            self.expect(&ArithToken::Colon)?;
            let false_val = self.parse_expression()?;
            Ok(if cond != 0 { true_val } else { false_val })
        } else {
            Ok(cond)
        }
    }

    fn parse_logical_or(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_logical_and()?;

        while self.peek() == Some(&ArithToken::LogicalOr) {
            self.advance();
            let right = self.parse_logical_and()?;
            left = if left != 0 || right != 0 { 1 } else { 0 };
        }

        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_bitwise_or()?;

        while self.peek() == Some(&ArithToken::LogicalAnd) {
            self.advance();
            let right = self.parse_bitwise_or()?;
            left = if left != 0 && right != 0 { 1 } else { 0 };
        }

        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_bitwise_xor()?;

        while self.peek() == Some(&ArithToken::BitwiseOr) {
            self.advance();
            let right = self.parse_bitwise_xor()?;
            left |= right;
        }

        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_bitwise_and()?;

        while self.peek() == Some(&ArithToken::BitwiseXor) {
            self.advance();
            let right = self.parse_bitwise_and()?;
            left ^= right;
        }

        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_equality()?;

        while self.peek() == Some(&ArithToken::BitwiseAnd) {
            self.advance();
            let right = self.parse_equality()?;
            left &= right;
        }

        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_comparison()?;

        loop {
            match self.peek() {
                Some(ArithToken::Equal) => {
                    self.advance();
                    let right = self.parse_comparison()?;
                    left = if left == right { 1 } else { 0 };
                }
                Some(ArithToken::NotEqual) => {
                    self.advance();
                    let right = self.parse_comparison()?;
                    left = if left != right { 1 } else { 0 };
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_shift()?;

        loop {
            match self.peek() {
                Some(ArithToken::Less) => {
                    self.advance();
                    let right = self.parse_shift()?;
                    left = if left < right { 1 } else { 0 };
                }
                Some(ArithToken::LessEqual) => {
                    self.advance();
                    let right = self.parse_shift()?;
                    left = if left <= right { 1 } else { 0 };
                }
                Some(ArithToken::Greater) => {
                    self.advance();
                    let right = self.parse_shift()?;
                    left = if left > right { 1 } else { 0 };
                }
                Some(ArithToken::GreaterEqual) => {
                    self.advance();
                    let right = self.parse_shift()?;
                    left = if left >= right { 1 } else { 0 };
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_additive()?;

        loop {
            match self.peek() {
                Some(ArithToken::LeftShift) => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left <<= right;
                }
                Some(ArithToken::RightShift) => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left >>= right;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_multiplicative()?;

        loop {
            match self.peek() {
                Some(ArithToken::Plus) => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left += right;
                }
                Some(ArithToken::Minus) => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left -= right;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_unary()?;

        loop {
            match self.peek() {
                Some(ArithToken::Multiply) => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left *= right;
                }
                Some(ArithToken::Divide) => {
                    self.advance();
                    let right = self.parse_unary()?;
                    if right == 0 {
                        return Err(ShellError::DivisionByZero);
                    }
                    left /= right;
                }
                Some(ArithToken::Modulo) => {
                    self.advance();
                    let right = self.parse_unary()?;
                    if right == 0 {
                        return Err(ShellError::DivisionByZero);
                    }
                    left %= right;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<i64, ShellError> {
        match self.peek() {
            Some(ArithToken::Minus) => {
                self.advance();
                let val = self.parse_unary()?;
                Ok(-val)
            }
            Some(ArithToken::LogicalNot) => {
                self.advance();
                let val = self.parse_unary()?;
                Ok(if val == 0 { 1 } else { 0 })
            }
            Some(ArithToken::BitwiseNot) => {
                self.advance();
                let val = self.parse_unary()?;
                Ok(!val)
            }
            Some(ArithToken::Increment) => {
                self.advance();
                let val = self.parse_primary()?;
                // TODO: Actually increment the variable
                Ok(val + 1)
            }
            Some(ArithToken::Decrement) => {
                self.advance();
                let val = self.parse_primary()?;
                // TODO: Actually decrement the variable
                Ok(val - 1)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<i64, ShellError> {
        match self.advance() {
            Some(ArithToken::Number(n)) => Ok(n),
            Some(ArithToken::Variable(name)) => {
                let value = self.env.get(&name).unwrap_or("0");
                value.parse::<i64>().map_err(|_| {
                    ShellError::ArithmeticSyntax(format!("invalid variable value: {}", value))
                })
            }
            Some(ArithToken::LeftParen) => {
                let val = self.parse_expression()?;
                self.expect(&ArithToken::RightParen)?;
                Ok(val)
            }
            Some(token) => Err(ShellError::ArithmeticSyntax(format!(
                "unexpected token: {:?}",
                token
            ))),
            None => Err(ShellError::ArithmeticSyntax(
                "unexpected end of expression".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic_basic() {
        let env = Env::new();
        assert_eq!(eval_arithmetic("1 + 2", &env).unwrap(), 3);
        assert_eq!(eval_arithmetic("5 - 3", &env).unwrap(), 2);
        assert_eq!(eval_arithmetic("4 * 3", &env).unwrap(), 12);
        assert_eq!(eval_arithmetic("10 / 2", &env).unwrap(), 5);
        assert_eq!(eval_arithmetic("10 % 3", &env).unwrap(), 1);
    }

    #[test]
    fn test_arithmetic_precedence() {
        let env = Env::new();
        assert_eq!(eval_arithmetic("2 + 3 * 4", &env).unwrap(), 14);
        assert_eq!(eval_arithmetic("(2 + 3) * 4", &env).unwrap(), 20);
        assert_eq!(eval_arithmetic("10 - 2 * 3", &env).unwrap(), 4);
    }

    #[test]
    fn test_arithmetic_comparison() {
        let env = Env::new();
        assert_eq!(eval_arithmetic("5 > 3", &env).unwrap(), 1);
        assert_eq!(eval_arithmetic("3 > 5", &env).unwrap(), 0);
        assert_eq!(eval_arithmetic("5 == 5", &env).unwrap(), 1);
        assert_eq!(eval_arithmetic("5 != 3", &env).unwrap(), 1);
    }

    #[test]
    fn test_arithmetic_logical() {
        let env = Env::new();
        assert_eq!(eval_arithmetic("1 && 1", &env).unwrap(), 1);
        assert_eq!(eval_arithmetic("1 && 0", &env).unwrap(), 0);
        assert_eq!(eval_arithmetic("0 || 1", &env).unwrap(), 1);
        assert_eq!(eval_arithmetic("!0", &env).unwrap(), 1);
    }

    #[test]
    fn test_arithmetic_bitwise() {
        let env = Env::new();
        assert_eq!(eval_arithmetic("0xFF & 0x0F", &env).unwrap(), 15);
        assert_eq!(eval_arithmetic("0xF0 | 0x0F", &env).unwrap(), 255);
        assert_eq!(eval_arithmetic("0xFF ^ 0x0F", &env).unwrap(), 240);
        assert_eq!(eval_arithmetic("1 << 4", &env).unwrap(), 16);
        assert_eq!(eval_arithmetic("16 >> 2", &env).unwrap(), 4);
    }

    #[test]
    fn test_arithmetic_variables() {
        let mut env = Env::new();
        env.set("X", "10");
        env.set("Y", "20");
        assert_eq!(eval_arithmetic("X + Y", &env).unwrap(), 30);
        assert_eq!(eval_arithmetic("$X * 2", &env).unwrap(), 20);
    }

    #[test]
    fn test_arithmetic_hex_octal_binary() {
        let env = Env::new();
        assert_eq!(eval_arithmetic("0xFF", &env).unwrap(), 255);
        assert_eq!(eval_arithmetic("0o77", &env).unwrap(), 63);
        assert_eq!(eval_arithmetic("0b1010", &env).unwrap(), 10);
    }

    #[test]
    fn test_arithmetic_ternary() {
        let env = Env::new();
        assert_eq!(eval_arithmetic("1 ? 10 : 20", &env).unwrap(), 10);
        assert_eq!(eval_arithmetic("0 ? 10 : 20", &env).unwrap(), 20);
    }
}
