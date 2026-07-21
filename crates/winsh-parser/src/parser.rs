//! The main parser implementation.

use winsh_ast::stmt::CaseItem;
use winsh_ast::token::TokenKind;
use winsh_ast::word::WordPart;
use winsh_ast::{RedirOp, RedirTarget, Redirection, Stmt, Token, Word};
use winsh_core::ShellError;

/// A parser for the WinSH shell language.
///
/// Converts a stream of tokens into an AST.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    /// Create a new parser for the given tokens.
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parse the tokens into a list of statements.
    pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, ShellError> {
        let mut parser = Self::new(tokens);
        let mut stmts = Vec::new();

        while !parser.is_at_end() {
            parser.skip_newlines();
            if parser.is_at_end() {
                break;
            }

            // Skip semicolons
            if parser.peek().kind == TokenKind::Semicolon {
                parser.advance();
                continue;
            }

            let stmt = parser.parse_statement()?;
            if !stmt.is_empty() {
                stmts.push(stmt);
            }

            // Skip optional semicolons and newlines after statements
            parser.skip_newlines();
            if parser.peek().kind == TokenKind::Semicolon {
                parser.advance();
            }
        }

        Ok(stmts)
    }

    /// Parse a single statement.
    fn parse_statement(&mut self) -> Result<Stmt, ShellError> {
        self.skip_newlines();

        if self.is_at_end() {
            return Ok(Stmt::Empty);
        }

        // Skip semicolons
        if self.peek().kind == TokenKind::Semicolon {
            self.advance();
            return Ok(Stmt::Empty);
        }

        // Parse the first part (could be a control structure or a pipeline)
        let stmt = match &self.peek().kind {
            TokenKind::If => self.parse_if()?,
            TokenKind::For => self.parse_for()?,
            TokenKind::While => self.parse_while()?,
            TokenKind::Until => self.parse_until()?,
            TokenKind::Case => self.parse_case()?,
            TokenKind::Select => self.parse_select()?,
            TokenKind::Function => self.parse_function()?,
            TokenKind::LeftBrace => self.parse_group()?,
            TokenKind::LeftParen => self.parse_subshell()?,
            _ => self.parse_pipeline()?,
        };

        // Handle && and || operators
        self.parse_and_or(stmt)
    }

    /// Parse && and || operators after a statement.
    fn parse_and_or(&mut self, left: Stmt) -> Result<Stmt, ShellError> {
        let mut current = left;

        loop {
            self.skip_newlines();

            match &self.peek().kind {
                TokenKind::And => {
                    self.advance();
                    self.skip_newlines();
                    let right = self.parse_simple_command()?;
                    current = Stmt::And {
                        left: Box::new(current),
                        right: Box::new(right),
                    };
                }
                TokenKind::Or => {
                    self.advance();
                    self.skip_newlines();
                    let right = self.parse_simple_command()?;
                    current = Stmt::Or {
                        left: Box::new(current),
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }

        Ok(current)
    }

    /// Parse a pipeline.
    fn parse_pipeline(&mut self) -> Result<Stmt, ShellError> {
        let mut negated = false;

        if self.peek().kind == TokenKind::Bang {
            self.advance();
            negated = true;
        }

        let mut commands = vec![self.parse_simple_command()?];

        while self.peek().kind == TokenKind::Pipe {
            self.advance();
            self.skip_newlines();
            commands.push(self.parse_simple_command()?);
        }

        if commands.len() == 1 && !negated {
            Ok(commands.pop().unwrap())
        } else {
            Ok(Stmt::Pipeline { commands, negated })
        }
    }

    /// Parse a simple command.
    fn parse_simple_command(&mut self) -> Result<Stmt, ShellError> {
        let mut words = Vec::new();
        let mut redirections = Vec::new();
        let mut background = false;

        loop {
            self.skip_newlines();

            if self.is_at_end() {
                break;
            }

            match &self.peek().kind {
                TokenKind::Pipe | TokenKind::And | TokenKind::Or | TokenKind::Semicolon => {
                    break;
                }
                TokenKind::Background => {
                    self.advance();
                    background = true;
                    break;
                }
                TokenKind::RedirOut
                | TokenKind::RedirIn
                | TokenKind::RedirAppend
                | TokenKind::RedirErr
                | TokenKind::RedirErrAppend
                | TokenKind::RedirErrToOut
                | TokenKind::RedirOutToErr
                | TokenKind::RedirCombined
                | TokenKind::RedirCombinedAppend
                | TokenKind::HereDoc
                | TokenKind::HereString => {
                    redirections.push(self.parse_redirection()?);
                }
                _ => {
                    words.push(self.parse_word()?);
                }
            }
        }

        if words.is_empty() && !redirections.is_empty() {
            // Pure redirection: > file.txt
            Ok(Stmt::Command {
                words: vec![Word::literal("echo")],
                redirections,
                background,
            })
        } else if words.is_empty() {
            Ok(Stmt::Empty)
        } else {
            Ok(Stmt::Command {
                words,
                redirections,
                background,
            })
        }
    }

    /// Parse a word.
    fn parse_word(&mut self) -> Result<Word, ShellError> {
        let token = self.advance();
        let span = token.span;

        let parts = match &token.kind {
            TokenKind::Word(s) => vec![WordPart::Literal(s.clone())],
            TokenKind::SingleQuoted(s) => vec![WordPart::SingleQuoted(s.clone())],
            TokenKind::DoubleQuoted(s) => vec![WordPart::Literal(s.clone())], // TODO: parse expansions
            TokenKind::DollarQuoted(s) => vec![WordPart::DollarQuoted(s.clone())],
            TokenKind::Variable(name) => vec![WordPart::Variable(name.clone())],
            TokenKind::BracedVariable(name) => vec![WordPart::BracedVariable(name.clone())],
            TokenKind::CommandSubst(cmd) => vec![WordPart::CommandSubst(cmd.clone())],
            TokenKind::BacktickSubst(cmd) => vec![WordPart::BacktickSubst(cmd.clone())],
            TokenKind::Arithmetic(expr) => vec![WordPart::Arithmetic(expr.clone())],
            TokenKind::Glob(pattern) => vec![WordPart::Glob(winsh_ast::word::GlobPattern {
                pattern: pattern.clone(),
                recursive: pattern.contains("**"),
            })],
            _ => {
                return Err(ShellError::unexpected_token(
                    token.kind.to_string(),
                    span.start,
                    0,
                ));
            }
        };

        Ok(Word { parts, span })
    }

    /// Parse a redirection.
    fn parse_redirection(&mut self) -> Result<Redirection, ShellError> {
        let token = self.advance();
        let op = match &token.kind {
            TokenKind::RedirIn => RedirOp::In,
            TokenKind::RedirOut => RedirOp::Out,
            TokenKind::RedirAppend => RedirOp::Append,
            TokenKind::RedirErr => RedirOp::Err,
            TokenKind::RedirErrAppend => RedirOp::ErrAppend,
            TokenKind::RedirErrToOut => RedirOp::ErrToOut,
            TokenKind::RedirOutToErr => RedirOp::OutToErr,
            TokenKind::RedirCombined => RedirOp::Combined,
            TokenKind::RedirCombinedAppend => RedirOp::CombinedAppend,
            TokenKind::HereDoc => RedirOp::HereDoc,
            TokenKind::HereString => RedirOp::HereString,
            _ => {
                return Err(ShellError::unexpected_token(
                    token.kind.to_string(),
                    token.span.start,
                    0,
                ));
            }
        };

        if op == RedirOp::ErrToOut {
            return Ok(Redirection {
                fd: Some(2),
                op,
                target: RedirTarget::Fd(1),
            });
        }

        if op == RedirOp::OutToErr {
            return Ok(Redirection {
                fd: Some(1),
                op,
                target: RedirTarget::Fd(2),
            });
        }

        self.skip_newlines();
        let target = self.parse_word()?;
        let fd = match op {
            RedirOp::Err | RedirOp::ErrAppend => Some(2),
            _ => None,
        };

        Ok(Redirection {
            fd,
            op,
            target: RedirTarget::File(target),
        })
    }

    /// Parse an if statement.
    fn parse_if(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip 'if'
        self.skip_newlines();

        let condition = Box::new(self.parse_statement()?);

        // Skip optional semicolons before 'then'
        self.skip_newlines();
        if self.peek().kind == TokenKind::Semicolon {
            self.advance();
        }

        self.expect_keyword(TokenKind::Then)?;
        self.skip_newlines();

        let mut then_branch = Vec::new();
        while self.peek().kind != TokenKind::Else
            && self.peek().kind != TokenKind::Elif
            && self.peek().kind != TokenKind::Fi
        {
            then_branch.push(self.parse_statement()?);
            self.skip_newlines();
            // Skip optional semicolons
            if self.peek().kind == TokenKind::Semicolon {
                self.advance();
            }
        }

        let mut elif_branches = Vec::new();
        while self.peek().kind == TokenKind::Elif {
            self.advance(); // Skip 'elif'
            self.skip_newlines();
            let elif_cond = Box::new(self.parse_statement()?);

            // Skip optional semicolons before 'then'
            self.skip_newlines();
            if self.peek().kind == TokenKind::Semicolon {
                self.advance();
            }

            self.expect_keyword(TokenKind::Then)?;
            self.skip_newlines();
            let mut elif_body = Vec::new();
            while self.peek().kind != TokenKind::Else
                && self.peek().kind != TokenKind::Elif
                && self.peek().kind != TokenKind::Fi
            {
                elif_body.push(self.parse_statement()?);
                self.skip_newlines();
                // Skip optional semicolons
                if self.peek().kind == TokenKind::Semicolon {
                    self.advance();
                }
            }
            elif_branches.push((*elif_cond, elif_body));
        }

        let else_branch = if self.peek().kind == TokenKind::Else {
            self.advance(); // Skip 'else'
            self.skip_newlines();
            let mut else_body = Vec::new();
            while self.peek().kind != TokenKind::Fi {
                else_body.push(self.parse_statement()?);
                self.skip_newlines();
                // Skip optional semicolons
                if self.peek().kind == TokenKind::Semicolon {
                    self.advance();
                }
            }
            Some(else_body)
        } else {
            None
        };

        self.expect_keyword(TokenKind::Fi)?;

        Ok(Stmt::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        })
    }

    /// Parse a for loop.
    fn parse_for(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip 'for'
        self.skip_newlines();

        // Check for C-style for loop: for ((...))
        if self.peek().kind == TokenKind::LeftParen {
            self.advance(); // Skip first (
            if self.peek().kind == TokenKind::LeftParen {
                self.advance(); // Skip second (
                return self.parse_c_style_for();
            }
            // Not a C-style for, backtrack
            self.pos -= 1;
        }

        let var = self.expect_word()?;
        self.skip_newlines();

        let mut words = Vec::new();
        if self.peek().kind == TokenKind::In {
            self.advance(); // Skip 'in'
            self.skip_newlines();
            while !self.is_at_end()
                && self.peek().kind != TokenKind::Semicolon
                && self.peek().kind != TokenKind::Do
            {
                words.push(self.parse_word()?);
            }
        }

        // Skip optional semicolons before 'do'
        self.skip_newlines();
        if self.peek().kind == TokenKind::Semicolon {
            self.advance();
        }

        self.expect_keyword(TokenKind::Do)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.peek().kind != TokenKind::Done {
            body.push(self.parse_statement()?);
            self.skip_newlines();
            // Skip optional semicolons
            if self.peek().kind == TokenKind::Semicolon {
                self.advance();
            }
        }

        self.expect_keyword(TokenKind::Done)?;

        Ok(Stmt::For { var, words, body })
    }

    /// Parse a C-style for loop.
    fn parse_c_style_for(&mut self) -> Result<Stmt, ShellError> {
        // TODO: Parse C-style for loop
        // For now, just consume until ))
        while !self.is_at_end() {
            if self.peek().kind == TokenKind::RightParen {
                self.advance();
                if self.peek().kind == TokenKind::RightParen {
                    self.advance();
                    break;
                }
            }
            self.advance();
        }

        self.skip_newlines();
        self.expect_keyword(TokenKind::Do)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.peek().kind != TokenKind::Done {
            body.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.expect_keyword(TokenKind::Done)?;

        Ok(Stmt::ForCStyle {
            init: None,
            condition: None,
            update: None,
            body,
        })
    }

    /// Parse a while loop.
    fn parse_while(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip 'while'
        self.skip_newlines();

        let condition = Box::new(self.parse_statement()?);

        // Skip optional semicolons before 'do'
        self.skip_newlines();
        if self.peek().kind == TokenKind::Semicolon {
            self.advance();
        }

        self.expect_keyword(TokenKind::Do)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.peek().kind != TokenKind::Done {
            body.push(self.parse_statement()?);
            self.skip_newlines();
            // Skip optional semicolons
            if self.peek().kind == TokenKind::Semicolon {
                self.advance();
            }
        }

        self.expect_keyword(TokenKind::Done)?;

        Ok(Stmt::While { condition, body })
    }

    /// Parse an until loop.
    fn parse_until(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip 'until'
        self.skip_newlines();

        let condition = Box::new(self.parse_statement()?);

        // Skip optional semicolons before 'do'
        self.skip_newlines();
        if self.peek().kind == TokenKind::Semicolon {
            self.advance();
        }

        self.expect_keyword(TokenKind::Do)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.peek().kind != TokenKind::Done {
            body.push(self.parse_statement()?);
            self.skip_newlines();
            // Skip optional semicolons
            if self.peek().kind == TokenKind::Semicolon {
                self.advance();
            }
        }

        self.expect_keyword(TokenKind::Done)?;

        Ok(Stmt::Until { condition, body })
    }

    /// Parse a case statement.
    fn parse_case(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip 'case'
        self.skip_newlines();

        let word = self.parse_word()?;

        self.skip_newlines();
        self.expect_keyword(TokenKind::In)?;
        self.skip_newlines();

        let mut cases = Vec::new();
        while self.peek().kind != TokenKind::Esac {
            if self.peek().kind == TokenKind::LeftParen {
                self.advance(); // Skip optional (
            }

            let mut patterns = vec![self.parse_word()?];
            while self.peek().kind == TokenKind::Pipe {
                self.advance();
                patterns.push(self.parse_word()?);
            }

            if self.peek().kind == TokenKind::RightParen {
                self.advance(); // Skip )
            }

            self.skip_newlines();

            let mut body = Vec::new();
            while self.peek().kind != TokenKind::Esac
                && self.peek().kind != TokenKind::LeftParen
                && !(self.peek().kind == TokenKind::Word(")".to_string()))
            {
                body.push(self.parse_statement()?);
                self.skip_newlines();
            }

            let fallthrough = if self.peek().kind == TokenKind::And {
                self.advance();
                true
            } else {
                if self.peek().kind == TokenKind::Semicolon {
                    self.advance();
                }
                false
            };

            cases.push(CaseItem {
                patterns,
                body,
                fallthrough,
            });
        }

        self.expect_keyword(TokenKind::Esac)?;

        Ok(Stmt::Case { word, cases })
    }

    /// Parse a select statement.
    fn parse_select(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip 'select'
        self.skip_newlines();

        let var = self.expect_word()?;
        self.skip_newlines();

        let mut words = Vec::new();
        if self.peek().kind == TokenKind::In {
            self.advance();
            self.skip_newlines();
            while !self.is_at_end()
                && self.peek().kind != TokenKind::Semicolon
                && self.peek().kind != TokenKind::Do
            {
                words.push(self.parse_word()?);
            }
        }

        self.skip_newlines();
        self.expect_keyword(TokenKind::Do)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.peek().kind != TokenKind::Done {
            body.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.expect_keyword(TokenKind::Done)?;

        Ok(Stmt::Select { var, words, body })
    }

    /// Parse a function definition.
    fn parse_function(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip 'function'
        self.skip_newlines();

        let name = self.expect_word()?;
        self.skip_newlines();

        // Optional ()
        if self.peek().kind == TokenKind::LeftParen {
            self.advance();
            if self.peek().kind == TokenKind::RightParen {
                self.advance();
            }
        }

        self.skip_newlines();

        // Expect { ... }
        self.expect_token(TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.peek().kind != TokenKind::RightBrace {
            body.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.advance(); // Skip }

        Ok(Stmt::FunctionDef { name, body })
    }

    /// Parse a group: { commands; }
    fn parse_group(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip {
        self.skip_newlines();

        let mut stmts = Vec::new();
        while self.peek().kind != TokenKind::RightBrace {
            stmts.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.advance(); // Skip }

        if stmts.len() == 1 {
            Ok(stmts.pop().unwrap())
        } else {
            Ok(Stmt::Group(Box::new(Stmt::Sequence(stmts))))
        }
    }

    /// Parse a subshell: ( commands )
    fn parse_subshell(&mut self) -> Result<Stmt, ShellError> {
        self.advance(); // Skip (
        self.skip_newlines();

        let mut stmts = Vec::new();
        while self.peek().kind != TokenKind::RightParen {
            stmts.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.advance(); // Skip )

        let inner = if stmts.len() == 1 {
            stmts.pop().unwrap()
        } else {
            Stmt::Sequence(stmts)
        };

        Ok(Stmt::Subshell(Box::new(inner)))
    }

    // Helper methods

    fn peek(&self) -> &Token {
        if self.is_at_end() {
            // Return a dummy EOF token
            static EOF_TOKEN: Token = Token {
                kind: TokenKind::Eof,
                span: winsh_ast::Span { start: 0, end: 0 },
            };
            &EOF_TOKEN
        } else {
            &self.tokens[self.pos]
        }
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        self.pos += 1;
        token
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.tokens[self.pos].kind == TokenKind::Eof
    }

    fn skip_newlines(&mut self) {
        while !self.is_at_end() && self.peek().kind == TokenKind::Newline {
            self.advance();
        }
    }

    fn expect_keyword(&mut self, keyword: TokenKind) -> Result<(), ShellError> {
        let token = self.advance();
        if token.kind != keyword {
            return Err(ShellError::unexpected_token(
                token.kind.to_string(),
                token.span.start,
                0,
            ));
        }
        Ok(())
    }

    fn expect_token(&mut self, expected: TokenKind) -> Result<Token, ShellError> {
        let token = self.advance();
        if token.kind != expected {
            return Err(ShellError::unexpected_token(
                token.kind.to_string(),
                token.span.start,
                0,
            ));
        }
        Ok(token)
    }

    fn expect_word(&mut self) -> Result<String, ShellError> {
        let token = self.advance();
        match &token.kind {
            TokenKind::Word(s) => Ok(s.clone()),
            _ => Err(ShellError::unexpected_token(
                token.kind.to_string(),
                token.span.start,
                0,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winsh_lexer::Lexer;

    fn parse(input: &str) -> Vec<Stmt> {
        let tokens = Lexer::tokenize(input).unwrap();
        Parser::parse(tokens).unwrap()
    }

    #[test]
    fn test_parse_simple_command() {
        let stmts = parse("echo hello");
        assert_eq!(stmts.len(), 1);
        assert!(stmts[0].is_command());
    }

    #[test]
    fn test_parse_pipeline() {
        let stmts = parse("ls | grep foo");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Pipeline { commands, negated } => {
                assert_eq!(commands.len(), 2);
                assert!(!negated);
            }
            _ => panic!("Expected pipeline"),
        }
    }

    #[test]
    fn test_parse_and_or() {
        let stmts = parse("a && b || c");
        assert_eq!(stmts.len(), 1);
        // Should be parsed as (a && b) || c
        match &stmts[0] {
            Stmt::Or { .. } => {}
            _ => panic!("Expected OR"),
        }
    }

    #[test]
    fn test_parse_if() {
        let stmts = parse("if true; then echo yes; else echo no; fi");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::If { .. } => {}
            _ => panic!("Expected if"),
        }
    }

    #[test]
    fn test_parse_for() {
        let stmts = parse("for i in 1 2 3; do echo $i; done");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::For { var, words, .. } => {
                assert_eq!(var, "i");
                assert_eq!(words.len(), 3);
            }
            _ => panic!("Expected for"),
        }
    }

    #[test]
    fn test_parse_while() {
        let stmts = parse("while true; do echo loop; done");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::While { .. } => {}
            _ => panic!("Expected while"),
        }
    }

    #[test]
    fn test_parse_function() {
        let stmts = parse("function greet() { echo hello; }");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::FunctionDef { name, .. } => {
                assert_eq!(name, "greet");
            }
            _ => panic!("Expected function"),
        }
    }

    #[test]
    fn test_parse_redirection() {
        let stmts = parse("echo hello > file.txt");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Command { redirections, .. } => {
                assert_eq!(redirections.len(), 1);
                assert_eq!(redirections[0].op, RedirOp::Out);
            }
            _ => panic!("Expected command with redirection"),
        }
    }

    #[test]
    fn test_parse_fd_redirections() {
        let stmts = parse("echo hello 2> err.txt 1>&2");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Command {
                words,
                redirections,
                ..
            } => {
                assert_eq!(words.len(), 2);
                assert_eq!(redirections.len(), 2);
                assert_eq!(redirections[0].fd, Some(2));
                assert_eq!(redirections[0].op, RedirOp::Err);
                assert_eq!(redirections[1].fd, Some(1));
                assert_eq!(redirections[1].op, RedirOp::OutToErr);
                assert_eq!(redirections[1].target, RedirTarget::Fd(2));
            }
            _ => panic!("Expected command with fd redirections"),
        }
    }

    #[test]
    fn test_parse_background() {
        let stmts = parse("sleep 10 &");
        assert_eq!(stmts.len(), 1);
        assert!(stmts[0].is_background());
    }

    #[test]
    fn test_parse_semicolon() {
        let stmts = parse("echo a; echo b");
        assert_eq!(stmts.len(), 2);
    }
}
