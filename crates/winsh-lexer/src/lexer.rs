//! The main lexer implementation.

use crate::quote;
use winsh_ast::token::TokenKind;
use winsh_ast::{Span, Token};
use winsh_core::ShellError;

/// A lexer for the WinSH shell language.
///
/// Converts raw input text into a stream of tokens.
pub struct Lexer {
    /// The input text
    input: Vec<char>,
    /// Current position in the input
    pos: usize,
    /// Current line number
    line: usize,
    /// Current column number
    col: usize,
    /// Start of the current token
    token_start: usize,
}

impl Lexer {
    /// Create a new lexer for the given input.
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            token_start: 0,
        }
    }

    /// Tokenize the entire input and return a vector of tokens.
    pub fn tokenize(input: &str) -> Result<Vec<Token>, ShellError> {
        let mut lexer = Self::new(input);
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    /// Get the next token from the input.
    pub fn next_token(&mut self) -> Result<Token, ShellError> {
        self.skip_whitespace();

        self.token_start = self.pos;

        if self.is_at_end() {
            return Ok(self.make_token(TokenKind::Eof));
        }

        let c = self.peek();

        // Handle comments
        if c == '#' && self.is_at_word_start() {
            return self.read_comment();
        }

        // Handle newlines
        if c == '\n' {
            self.advance();
            return Ok(self.make_token(TokenKind::Newline));
        }

        if c == '1' || c == '2' {
            if let Some(token) = self.read_fd_redirection(c) {
                return Ok(token);
            }
        }

        // Handle operators
        match c {
            '|' => {
                self.advance();
                if self.peek() == '|' {
                    self.advance();
                    return Ok(self.make_token(TokenKind::Or));
                }
                return Ok(self.make_token(TokenKind::Pipe));
            }
            '&' => {
                self.advance();
                if self.peek() == '&' {
                    self.advance();
                    return Ok(self.make_token(TokenKind::And));
                }
                if self.peek() == '>' {
                    self.advance();
                    if self.peek() == '>' {
                        self.advance();
                        return Ok(self.make_token(TokenKind::RedirCombinedAppend));
                    }
                    return Ok(self.make_token(TokenKind::RedirCombined));
                }
                return Ok(self.make_token(TokenKind::Background));
            }
            ';' => {
                self.advance();
                return Ok(self.make_token(TokenKind::Semicolon));
            }
            '(' => {
                self.advance();
                return Ok(self.make_token(TokenKind::LeftParen));
            }
            ')' => {
                self.advance();
                return Ok(self.make_token(TokenKind::RightParen));
            }
            '{' => {
                self.advance();
                return Ok(self.make_token(TokenKind::LeftBrace));
            }
            '}' => {
                self.advance();
                return Ok(self.make_token(TokenKind::RightBrace));
            }
            '<' => {
                self.advance();
                if self.peek() == '<' {
                    self.advance();
                    if self.peek() == '<' {
                        self.advance();
                        return Ok(self.make_token(TokenKind::HereString));
                    }
                    return Ok(self.make_token(TokenKind::HereDoc));
                }
                if self.peek() == '&' {
                    self.advance();
                    if self.peek() == '-' {
                        self.advance();
                        return Ok(self.make_token(TokenKind::RedirIn));
                    }
                    return Ok(self.make_token(TokenKind::RedirIn));
                }
                return Ok(self.make_token(TokenKind::RedirIn));
            }
            '>' => {
                self.advance();
                if self.peek() == '>' {
                    self.advance();
                    return Ok(self.make_token(TokenKind::RedirAppend));
                }
                if self.peek() == '&' {
                    self.advance();
                    return Ok(self.make_token(TokenKind::RedirOut));
                }
                return Ok(self.make_token(TokenKind::RedirOut));
            }
            '!' => {
                self.advance();
                return Ok(self.make_token(TokenKind::Bang));
            }
            '$' => {
                return self.read_dollar();
            }
            '\'' => {
                return self.read_single_quote();
            }
            '"' => {
                return self.read_double_quote();
            }
            '`' => {
                return self.read_backtick();
            }
            '\\' => {
                self.advance();
                if self.is_at_end() {
                    return Err(ShellError::unterminated("escape sequence", self.line));
                }
                let escaped = self.advance();
                return Ok(self.make_token(TokenKind::Word(format!("\\{}", escaped))));
            }
            '[' => {
                self.advance();
                if self.peek() == '[' {
                    self.advance();
                    return Ok(self.make_token(TokenKind::DoubleLeftBracket));
                }
                return self.read_bracket_pattern();
            }
            ']' => {
                self.advance();
                return Ok(self.make_token(TokenKind::Word("]".to_string())));
            }
            '~' => {
                return self.read_tilde();
            }
            _ => {
                return self.read_word();
            }
        }
    }

    /// Read a word (unquoted text).
    fn read_word(&mut self) -> Result<Token, ShellError> {
        let mut word = String::new();

        while !self.is_at_end() {
            let c = self.peek();

            // Stop at whitespace or special characters
            if c.is_whitespace() || "|&;()<>!#`'\"".contains(c) {
                break;
            }

            // Handle variable expansion
            if c == '$' {
                break;
            }

            // Handle glob characters
            if quote::is_glob_char(c) {
                // Include glob characters in the word
                word.push(self.advance());
                continue;
            }

            // Handle tilde expansion at word start
            if c == '~' && word.is_empty() {
                break;
            }

            word.push(self.advance());
        }

        if word.is_empty() {
            return Err(ShellError::UnexpectedEof);
        }

        // Check if this is a keyword
        if let Some(keyword) = winsh_ast::token::word_to_keyword(&word) {
            return Ok(self.make_token(keyword));
        }

        Ok(self.make_token(TokenKind::Word(word)))
    }

    /// Read a single-quoted string.
    fn read_single_quote(&mut self) -> Result<Token, ShellError> {
        self.advance(); // Skip opening quote
        let start_line = self.line;
        let mut content = String::new();

        while !self.is_at_end() && self.peek() != '\'' {
            content.push(self.advance());
        }

        if self.is_at_end() {
            return Err(ShellError::unterminated("single quote", start_line));
        }

        self.advance(); // Skip closing quote
        Ok(self.make_token(TokenKind::SingleQuoted(content)))
    }

    /// Read a double-quoted string.
    fn read_double_quote(&mut self) -> Result<Token, ShellError> {
        self.advance(); // Skip opening quote
        let start_line = self.line;
        let mut content = String::new();

        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '\\' {
                self.advance();
                if self.is_at_end() {
                    return Err(ShellError::unterminated("double quote", start_line));
                }
                let escaped = self.advance();
                // In double quotes, only certain characters are escaped
                match escaped {
                    '"' | '\\' | '$' | '`' | '!' => content.push(escaped),
                    '\n' => {
                        // Line continuation - skip the newline
                        continue;
                    }
                    _ => {
                        content.push('\\');
                        content.push(escaped);
                    }
                }
            } else if self.peek() == '$' {
                // TODO: Handle variable expansion inside double quotes
                content.push(self.advance());
            } else if self.peek() == '`' {
                // TODO: Handle command substitution inside double quotes
                content.push(self.advance());
            } else {
                content.push(self.advance());
            }
        }

        if self.is_at_end() {
            return Err(ShellError::unterminated("double quote", start_line));
        }

        self.advance(); // Skip closing quote
        Ok(self.make_token(TokenKind::DoubleQuoted(content)))
    }

    /// Read a dollar-quoted string.
    fn read_dollar_quote(&mut self) -> Result<Token, ShellError> {
        self.advance(); // Skip opening $'
        let start_line = self.line;
        let mut content = String::new();

        while !self.is_at_end() && self.peek() != '\'' {
            content.push(self.advance());
        }

        if self.is_at_end() {
            return Err(ShellError::unterminated("dollar quote", start_line));
        }

        self.advance(); // Skip closing quote
        let processed = quote::process_dollar_quotes(&content)?;
        Ok(self.make_token(TokenKind::DollarQuoted(processed)))
    }

    /// Read a dollar sign (variable, command substitution, or arithmetic).
    fn read_dollar(&mut self) -> Result<Token, ShellError> {
        self.advance(); // Skip $

        if self.is_at_end() {
            return Ok(self.make_token(TokenKind::Dollar));
        }

        match self.peek() {
            '(' => {
                self.advance(); // Skip (
                if self.peek() == '(' {
                    // Arithmetic expansion: $((...))
                    self.advance(); // Skip second (
                    return self.read_until_delimiter("))", TokenKind::Arithmetic);
                }
                // Command substitution: $(...)
                return self.read_until_delimiter(")", TokenKind::CommandSubst);
            }
            '{' => {
                // Braced variable: ${VAR}
                self.advance(); // Skip {
                return self.read_braced_variable();
            }
            '\'' => {
                // Dollar-quoted string: $'...'
                return self.read_dollar_quote();
            }
            _ => {
                // Simple variable: $VAR
                return self.read_variable();
            }
        }
    }

    /// Read a variable name.
    fn read_variable(&mut self) -> Result<Token, ShellError> {
        let mut name = String::new();

        // Special variables: $?, $!, $#, $$, $-, $0, $1-$9, $@
        if !self.is_at_end() && self.peek() == '?' {
            self.advance();
            return Ok(self.make_token(TokenKind::Variable("?".to_string())));
        }
        if !self.is_at_end() && self.peek() == '!' {
            self.advance();
            return Ok(self.make_token(TokenKind::Variable("!".to_string())));
        }
        if !self.is_at_end() && self.peek() == '#' {
            self.advance();
            return Ok(self.make_token(TokenKind::Variable("#".to_string())));
        }
        if !self.is_at_end() && self.peek() == '$' {
            self.advance();
            return Ok(self.make_token(TokenKind::Variable("$".to_string())));
        }
        if !self.is_at_end() && self.peek() == '-' {
            self.advance();
            return Ok(self.make_token(TokenKind::Variable("-".to_string())));
        }
        if !self.is_at_end() && self.peek() == '@' {
            self.advance();
            return Ok(self.make_token(TokenKind::Variable("@".to_string())));
        }
        if !self.is_at_end() && self.peek() == '*' {
            self.advance();
            return Ok(self.make_token(TokenKind::Variable("*".to_string())));
        }

        // Read variable name (alphanumeric + underscore)
        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            name.push(self.advance());
        }

        if name.is_empty() {
            return Ok(self.make_token(TokenKind::Dollar));
        }

        Ok(self.make_token(TokenKind::Variable(name)))
    }

    /// Read a braced variable: ${VAR}
    fn read_braced_variable(&mut self) -> Result<Token, ShellError> {
        let mut name = String::new();
        let start_line = self.line;

        // Check for ${#VAR} (length)
        if !self.is_at_end() && self.peek() == '#' {
            self.advance();
            while !self.is_at_end() && self.peek() != '}' {
                name.push(self.advance());
            }
            if self.is_at_end() {
                return Err(ShellError::unterminated("braced variable", start_line));
            }
            self.advance(); // Skip }
            return Ok(self.make_token(TokenKind::BracedVariable(format!("#{}", name))));
        }

        // Read variable name
        while !self.is_at_end()
            && self.peek() != '}'
            && self.peek() != ':'
            && self.peek() != '-'
            && self.peek() != '='
            && self.peek() != '+'
            && self.peek() != '?'
            && self.peek() != '#'
            && self.peek() != '%'
            && self.peek() != '/'
        {
            name.push(self.advance());
        }

        // Check for parameter expansion operators
        if !self.is_at_end() && self.peek() == ':' {
            // ${VAR:-default}, ${VAR:=default}, ${VAR:+alternate}, ${VAR:?error}
            // For now, just read until }
            let mut full = name.clone();
            full.push(self.advance()); // :
            while !self.is_at_end() && self.peek() != '}' {
                full.push(self.advance());
            }
            if self.is_at_end() {
                return Err(ShellError::unterminated("braced variable", start_line));
            }
            self.advance(); // Skip }
            return Ok(self.make_token(TokenKind::BracedVariable(full)));
        }

        if !self.is_at_end() && (self.peek() == '#' || self.peek() == '%' || self.peek() == '/') {
            // ${VAR#pattern}, ${VAR%pattern}, ${VAR/old/new}
            let mut full = name.clone();
            full.push(self.advance()); // operator
            if !self.is_at_end() && (self.peek() == '#' || self.peek() == '%' || self.peek() == '/')
            {
                full.push(self.advance()); // double operator
            }
            while !self.is_at_end() && self.peek() != '}' {
                full.push(self.advance());
            }
            if self.is_at_end() {
                return Err(ShellError::unterminated("braced variable", start_line));
            }
            self.advance(); // Skip }
            return Ok(self.make_token(TokenKind::BracedVariable(full)));
        }

        if self.is_at_end() {
            return Err(ShellError::unterminated("braced variable", start_line));
        }

        self.advance(); // Skip }
        Ok(self.make_token(TokenKind::BracedVariable(name)))
    }

    /// Read a backtick command substitution.
    fn read_backtick(&mut self) -> Result<Token, ShellError> {
        self.advance(); // Skip opening `
        let start_line = self.line;
        let mut content = String::new();

        while !self.is_at_end() && self.peek() != '`' {
            if self.peek() == '\\' {
                self.advance();
                if !self.is_at_end() {
                    content.push(self.advance());
                }
            } else {
                content.push(self.advance());
            }
        }

        if self.is_at_end() {
            return Err(ShellError::unterminated("backtick", start_line));
        }

        self.advance(); // Skip closing `
        Ok(self.make_token(TokenKind::BacktickSubst(content)))
    }

    /// Read until a delimiter and create a token.
    fn read_until_delimiter(
        &mut self,
        delimiter: &str,
        kind_fn: fn(String) -> TokenKind,
    ) -> Result<Token, ShellError> {
        let start_line = self.line;
        let mut content = String::new();
        let delimiter_chars: Vec<char> = delimiter.chars().collect();
        let mut depth = 1;

        while !self.is_at_end() {
            if self.peek() == delimiter_chars[0] {
                // Check if we have the full delimiter
                let mut matched = true;
                for &dc in &delimiter_chars {
                    if self.is_at_end() || self.peek() != dc {
                        matched = false;
                        break;
                    }
                    self.advance();
                }
                if matched {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(self.make_token(kind_fn(content)));
                    }
                    content.extend(delimiter_chars.iter());
                }
            } else {
                if self.peek() == '(' {
                    depth += 1;
                }
                content.push(self.advance());
            }
        }

        Err(ShellError::unterminated("substitution", start_line))
    }

    /// Read a bracket pattern [...]
    fn read_bracket_pattern(&mut self) -> Result<Token, ShellError> {
        let mut pattern = String::from("[");
        let start_line = self.line;

        // Check for negation
        if !self.is_at_end() && self.peek() == '!' {
            pattern.push(self.advance());
        }

        // Read until closing bracket
        while !self.is_at_end() && self.peek() != ']' {
            if self.peek() == '\\' {
                pattern.push(self.advance());
                if !self.is_at_end() {
                    pattern.push(self.advance());
                }
            } else {
                pattern.push(self.advance());
            }
        }

        if self.is_at_end() {
            return Err(ShellError::unterminated("bracket pattern", start_line));
        }

        pattern.push(self.advance()); // Skip ]
        Ok(self.make_token(TokenKind::Glob(pattern)))
    }

    /// Read a tilde expansion.
    fn read_tilde(&mut self) -> Result<Token, ShellError> {
        self.advance(); // Skip ~
        let mut word = String::from("~");

        while !self.is_at_end() {
            let c = self.peek();
            if c.is_whitespace() || "|&;()<>!#`'\"$".contains(c) {
                break;
            }
            word.push(self.advance());
        }

        Ok(self.make_token(TokenKind::Word(word)))
    }

    /// Read fd-qualified redirections such as 2>, 2>>, 2>&1, and 1>&2.
    fn read_fd_redirection(&mut self, fd: char) -> Option<Token> {
        if self.peek_offset(1) != '>' {
            return None;
        }

        match fd {
            '1' if self.peek_offset(2) == '&' && self.peek_offset(3) == '2' => {
                self.advance();
                self.advance();
                self.advance();
                self.advance();
                Some(self.make_token(TokenKind::RedirOutToErr))
            }
            '2' if self.peek_offset(2) == '&' && self.peek_offset(3) == '1' => {
                self.advance();
                self.advance();
                self.advance();
                self.advance();
                Some(self.make_token(TokenKind::RedirErrToOut))
            }
            '2' if self.peek_offset(2) == '>' => {
                self.advance();
                self.advance();
                self.advance();
                Some(self.make_token(TokenKind::RedirErrAppend))
            }
            '2' => {
                self.advance();
                self.advance();
                Some(self.make_token(TokenKind::RedirErr))
            }
            _ => None,
        }
    }

    /// Read a comment.
    fn read_comment(&mut self) -> Result<Token, ShellError> {
        self.advance(); // Skip #
        let mut comment = String::new();

        while !self.is_at_end() && self.peek() != '\n' {
            comment.push(self.advance());
        }

        Ok(self.make_token(TokenKind::Comment(comment)))
    }

    /// Skip whitespace (excluding newlines).
    fn skip_whitespace(&mut self) {
        while !self.is_at_end()
            && (self.peek() == ' ' || self.peek() == '\t' || self.peek() == '\r')
        {
            self.advance();
        }
    }

    /// Check if we're at the start of a word (for comment detection).
    fn is_at_word_start(&self) -> bool {
        self.pos == 0
            || self.input[self.pos - 1] == '\n'
            || self.input[self.pos - 1] == ' '
            || self.input[self.pos - 1] == '\t'
            || self.input[self.pos - 1] == ';'
            || self.input[self.pos - 1] == '|'
            || self.input[self.pos - 1] == '&'
            || self.input[self.pos - 1] == '('
    }

    /// Peek at the current character without consuming it.
    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.input[self.pos]
        }
    }

    /// Peek ahead without consuming input.
    fn peek_offset(&self, offset: usize) -> char {
        self.input.get(self.pos + offset).copied().unwrap_or('\0')
    }

    /// Advance to the next character and return the previous one.
    fn advance(&mut self) -> char {
        let c = self.input[self.pos];
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    /// Check if we've reached the end of the input.
    fn is_at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Create a token with the current span.
    fn make_token(&self, kind: TokenKind) -> Token {
        Token {
            kind,
            span: Span::new(self.token_start, self.pos),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_command() {
        let tokens = Lexer::tokenize("echo hello").unwrap();
        assert_eq!(tokens.len(), 3); // echo, hello, EOF
        assert_eq!(tokens[0].kind, TokenKind::Word("echo".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Word("hello".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Eof);
    }

    #[test]
    fn test_tokenize_pipe() {
        let tokens = Lexer::tokenize("ls | grep foo").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("ls".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Pipe);
        assert_eq!(tokens[2].kind, TokenKind::Word("grep".to_string()));
        assert_eq!(tokens[3].kind, TokenKind::Word("foo".to_string()));
    }

    #[test]
    fn test_tokenize_operators() {
        let tokens = Lexer::tokenize("a && b || c ; d &").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("a".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::And);
        assert_eq!(tokens[2].kind, TokenKind::Word("b".to_string()));
        assert_eq!(tokens[3].kind, TokenKind::Or);
        assert_eq!(tokens[4].kind, TokenKind::Word("c".to_string()));
        assert_eq!(tokens[5].kind, TokenKind::Semicolon);
        assert_eq!(tokens[6].kind, TokenKind::Word("d".to_string()));
        assert_eq!(tokens[7].kind, TokenKind::Background);
    }

    #[test]
    fn test_tokenize_redirections() {
        let tokens = Lexer::tokenize("echo hello > file.txt").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("echo".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Word("hello".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::RedirOut);
        assert_eq!(tokens[3].kind, TokenKind::Word("file.txt".to_string()));
    }

    #[test]
    fn test_tokenize_fd_redirections() {
        let tokens = Lexer::tokenize("echo hello 2> err.txt 1>&2").unwrap();
        assert_eq!(tokens[2].kind, TokenKind::RedirErr);
        assert_eq!(tokens[3].kind, TokenKind::Word("err.txt".to_string()));
        assert_eq!(tokens[4].kind, TokenKind::RedirOutToErr);

        let tokens = Lexer::tokenize("cmd 2>> err.txt 2>&1").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::RedirErrAppend);
        assert_eq!(tokens[3].kind, TokenKind::RedirErrToOut);
    }

    #[test]
    fn test_tokenize_single_quote() {
        let tokens = Lexer::tokenize("echo 'hello world'").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("echo".to_string()));
        assert_eq!(
            tokens[1].kind,
            TokenKind::SingleQuoted("hello world".to_string())
        );
    }

    #[test]
    fn test_tokenize_double_quote() {
        let tokens = Lexer::tokenize("echo \"hello world\"").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("echo".to_string()));
        assert_eq!(
            tokens[1].kind,
            TokenKind::DoubleQuoted("hello world".to_string())
        );
    }

    #[test]
    fn test_tokenize_variable() {
        let tokens = Lexer::tokenize("echo $HOME").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("echo".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Variable("HOME".to_string()));
    }

    #[test]
    fn test_tokenize_tilde_path_as_single_word() {
        let tokens = Lexer::tokenize("cd ~/Desktop").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("cd".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Word("~/Desktop".to_string()));
    }

    #[test]
    fn test_tokenize_braced_variable() {
        let tokens = Lexer::tokenize("echo ${HOME}").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("echo".to_string()));
        assert_eq!(
            tokens[1].kind,
            TokenKind::BracedVariable("HOME".to_string())
        );
    }

    #[test]
    fn test_tokenize_keywords() {
        let tokens = Lexer::tokenize("if then else fi").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::If);
        assert_eq!(tokens[1].kind, TokenKind::Then);
        assert_eq!(tokens[2].kind, TokenKind::Else);
        assert_eq!(tokens[3].kind, TokenKind::Fi);
    }

    #[test]
    fn test_tokenize_comment() {
        let tokens = Lexer::tokenize("echo hello # this is a comment").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Word("echo".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Word("hello".to_string()));
        assert_eq!(
            tokens[2].kind,
            TokenKind::Comment(" this is a comment".to_string())
        );
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = Lexer::tokenize("").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_tokenize_whitespace_only() {
        let tokens = Lexer::tokenize("   ").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_tokenize_unterminated_single_quote() {
        let result = Lexer::tokenize("echo 'hello");
        assert!(result.is_err());
    }

    #[test]
    fn test_tokenize_unterminated_double_quote() {
        let result = Lexer::tokenize("echo \"hello");
        assert!(result.is_err());
    }
}
