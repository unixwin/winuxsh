//! Statement types for the AST.

use crate::expr::Expr;
use crate::redir::Redirection;
use crate::word::Word;
use std::fmt;

/// A statement in the shell language.
///
/// Statements are the top-level constructs that can be executed.
/// A script is a sequence of statements.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// A simple command with arguments and redirections
    Command {
        /// The command name/words
        words: Vec<Word>,
        /// Input redirections
        redirections: Vec<Redirection>,
        /// Whether to run in background
        background: bool,
    },

    /// A pipeline of commands: cmd1 | cmd2 | cmd3
    Pipeline {
        commands: Vec<Stmt>,
        /// Whether the pipeline is negated: ! cmd1 | cmd2
        negated: bool,
    },

    /// Logical AND: cmd1 && cmd2
    And { left: Box<Stmt>, right: Box<Stmt> },

    /// Logical OR: cmd1 || cmd2
    Or { left: Box<Stmt>, right: Box<Stmt> },

    /// Sequence of statements: cmd1 ; cmd2 ; cmd3
    Sequence(Vec<Stmt>),

    /// Subshell: ( commands )
    Subshell(Box<Stmt>),

    /// Grouping: { commands; }
    Group(Box<Stmt>),

    /// If statement: if ... then ... elif ... else ... fi
    If {
        condition: Box<Stmt>,
        then_branch: Vec<Stmt>,
        elif_branches: Vec<(Stmt, Vec<Stmt>)>,
        else_branch: Option<Vec<Stmt>>,
    },

    /// For loop: for VAR in WORDS; do ...; done
    For {
        var: String,
        words: Vec<Word>,
        body: Vec<Stmt>,
    },

    /// C-style for loop: for ((init; cond; update)); do ...; done
    ForCStyle {
        init: Option<Box<Expr>>,
        condition: Option<Box<Expr>>,
        update: Option<Box<Expr>>,
        body: Vec<Stmt>,
    },

    /// While loop: while COND; do ...; done
    While {
        condition: Box<Stmt>,
        body: Vec<Stmt>,
    },

    /// Until loop: until COND; do ...; done
    Until {
        condition: Box<Stmt>,
        body: Vec<Stmt>,
    },

    /// Case statement: case WORD in PATTERN) COMMANDS ;; ... esac
    Case { word: Word, cases: Vec<CaseItem> },

    /// Select statement: select VAR in WORDS; do ...; done
    Select {
        var: String,
        words: Vec<Word>,
        body: Vec<Stmt>,
    },

    /// Function definition: name() { ... } or function name { ... }
    FunctionDef { name: String, body: Vec<Stmt> },

    /// Arithmetic evaluation: (( expr ))
    ArithmeticEval(Box<Expr>),

    /// Conditional expression: [[ expr ]]
    Conditional(Box<Expr>),

    /// Variable assignment: VAR=value
    Assign {
        name: String,
        value: Word,
        /// Whether to export the variable
        export: bool,
        /// Whether the variable is local
        local: bool,
        /// Whether the variable is readonly
        readonly: bool,
    },

    /// Here document
    HereDoc {
        delimiter: String,
        content: String,
        strip_tabs: bool,
    },

    /// Empty statement (just a semicolon or newline)
    Empty,
}

/// A case item in a case statement.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseItem {
    /// The patterns to match
    pub patterns: Vec<Word>,
    /// The commands to execute if matched
    pub body: Vec<Stmt>,
    /// Whether this is the last item (no ;;)
    pub fallthrough: bool,
}

impl Stmt {
    /// Check if this statement is empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, Stmt::Empty)
    }

    /// Check if this statement is a simple command.
    pub fn is_command(&self) -> bool {
        matches!(self, Stmt::Command { .. })
    }

    /// Get the command words if this is a simple command.
    pub fn as_command(&self) -> Option<&[Word]> {
        if let Stmt::Command { words, .. } = self {
            Some(words)
        } else {
            None
        }
    }

    /// Check if this statement runs in background.
    pub fn is_background(&self) -> bool {
        matches!(
            self,
            Stmt::Command {
                background: true,
                ..
            }
        )
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Command {
                words,
                redirections,
                background,
            } => {
                for (i, word) in words.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", word)?;
                }
                for redir in redirections {
                    write!(f, " {}", redir)?;
                }
                if *background {
                    write!(f, " &")?;
                }
                Ok(())
            }
            Stmt::Pipeline { commands, negated } => {
                if *negated {
                    write!(f, "! ")?;
                }
                for (i, cmd) in commands.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", cmd)?;
                }
                Ok(())
            }
            Stmt::And { left, right } => write!(f, "{} && {}", left, right),
            Stmt::Or { left, right } => write!(f, "{} || {}", left, right),
            Stmt::Sequence(stmts) => {
                for (i, stmt) in stmts.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{}", stmt)?;
                }
                Ok(())
            }
            Stmt::Subshell(stmt) => write!(f, "({})", stmt),
            Stmt::Group(stmt) => write!(f, "{{ {}; }}", stmt),
            Stmt::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                write!(f, "if {}; then ", condition)?;
                for stmt in then_branch {
                    write!(f, "{}; ", stmt)?;
                }
                for (cond, body) in elif_branches {
                    write!(f, "elif {}; then ", cond)?;
                    for stmt in body {
                        write!(f, "{}; ", stmt)?;
                    }
                }
                if let Some(else_body) = else_branch {
                    write!(f, "else ")?;
                    for stmt in else_body {
                        write!(f, "{}; ", stmt)?;
                    }
                }
                write!(f, "fi")
            }
            Stmt::For { var, words, body } => {
                write!(f, "for {} in", var)?;
                for word in words {
                    write!(f, " {}", word)?;
                }
                write!(f, "; do ")?;
                for stmt in body {
                    write!(f, "{}; ", stmt)?;
                }
                write!(f, "done")
            }
            Stmt::ForCStyle {
                init,
                condition,
                update,
                body,
            } => {
                write!(f, "for ((")?;
                if let Some(i) = init {
                    write!(f, "{}", i)?;
                }
                write!(f, "; ")?;
                if let Some(c) = condition {
                    write!(f, "{}", c)?;
                }
                write!(f, "; ")?;
                if let Some(u) = update {
                    write!(f, "{}", u)?;
                }
                write!(f, ")); do ")?;
                for stmt in body {
                    write!(f, "{}; ", stmt)?;
                }
                write!(f, "done")
            }
            Stmt::While { condition, body } => {
                write!(f, "while {}; do ", condition)?;
                for stmt in body {
                    write!(f, "{}; ", stmt)?;
                }
                write!(f, "done")
            }
            Stmt::Until { condition, body } => {
                write!(f, "until {}; do ", condition)?;
                for stmt in body {
                    write!(f, "{}; ", stmt)?;
                }
                write!(f, "done")
            }
            Stmt::Case { word, cases } => {
                write!(f, "case {} in ", word)?;
                for item in cases {
                    for (i, pattern) in item.patterns.iter().enumerate() {
                        if i > 0 {
                            write!(f, " | ")?;
                        }
                        write!(f, "{}", pattern)?;
                    }
                    write!(f, ") ")?;
                    for stmt in &item.body {
                        write!(f, "{}; ", stmt)?;
                    }
                    if !item.fallthrough {
                        write!(f, ";; ")?;
                    }
                }
                write!(f, "esac")
            }
            Stmt::Select { var, words, body } => {
                write!(f, "select {} in", var)?;
                for word in words {
                    write!(f, " {}", word)?;
                }
                write!(f, "; do ")?;
                for stmt in body {
                    write!(f, "{}; ", stmt)?;
                }
                write!(f, "done")
            }
            Stmt::FunctionDef { name, body } => {
                write!(f, "{}() {{ ", name)?;
                for stmt in body {
                    write!(f, "{}; ", stmt)?;
                }
                write!(f, "}}")
            }
            Stmt::ArithmeticEval(expr) => write!(f, "(( {} ))", expr),
            Stmt::Conditional(expr) => write!(f, "[[ {} ]]", expr),
            Stmt::Assign { name, value, .. } => write!(f, "{}={}", name, value),
            Stmt::HereDoc {
                delimiter, content, ..
            } => {
                write!(f, "<<{}\n{}\n{}", delimiter, content, delimiter)
            }
            Stmt::Empty => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stmt_command() {
        let stmt = Stmt::Command {
            words: vec![Word::literal("echo"), Word::literal("hello")],
            redirections: vec![],
            background: false,
        };
        assert!(stmt.is_command());
        assert!(!stmt.is_background());
    }

    #[test]
    fn test_stmt_pipeline() {
        let stmt = Stmt::Pipeline {
            commands: vec![
                Stmt::Command {
                    words: vec![Word::literal("ls")],
                    redirections: vec![],
                    background: false,
                },
                Stmt::Command {
                    words: vec![Word::literal("grep"), Word::literal("foo")],
                    redirections: vec![],
                    background: false,
                },
            ],
            negated: false,
        };
        assert_eq!(stmt.to_string(), "ls | grep foo");
    }

    #[test]
    fn test_stmt_and() {
        let stmt = Stmt::And {
            left: Box::new(Stmt::Command {
                words: vec![Word::literal("true")],
                redirections: vec![],
                background: false,
            }),
            right: Box::new(Stmt::Command {
                words: vec![Word::literal("echo"), Word::literal("ok")],
                redirections: vec![],
                background: false,
            }),
        };
        assert_eq!(stmt.to_string(), "true && echo ok");
    }

    #[test]
    fn test_stmt_if() {
        let stmt = Stmt::If {
            condition: Box::new(Stmt::Command {
                words: vec![
                    Word::literal("test"),
                    Word::literal("-f"),
                    Word::literal("file"),
                ],
                redirections: vec![],
                background: false,
            }),
            then_branch: vec![Stmt::Command {
                words: vec![Word::literal("echo"), Word::literal("exists")],
                redirections: vec![],
                background: false,
            }],
            elif_branches: vec![],
            else_branch: Some(vec![Stmt::Command {
                words: vec![Word::literal("echo"), Word::literal("not found")],
                redirections: vec![],
                background: false,
            }]),
        };
        let s = stmt.to_string();
        assert!(s.contains("if"));
        assert!(s.contains("then"));
        assert!(s.contains("else"));
        assert!(s.contains("fi"));
    }

    #[test]
    fn test_stmt_for() {
        let stmt = Stmt::For {
            var: "i".to_string(),
            words: vec![Word::literal("1"), Word::literal("2"), Word::literal("3")],
            body: vec![Stmt::Command {
                words: vec![Word::literal("echo"), Word::variable("i")],
                redirections: vec![],
                background: false,
            }],
        };
        let s = stmt.to_string();
        assert!(s.contains("for i in"));
        assert!(s.contains("do"));
        assert!(s.contains("done"));
    }

    #[test]
    fn test_case_item() {
        let item = CaseItem {
            patterns: vec![Word::literal("*.txt")],
            body: vec![Stmt::Command {
                words: vec![Word::literal("echo"), Word::literal("text file")],
                redirections: vec![],
                background: false,
            }],
            fallthrough: false,
        };
        assert_eq!(item.patterns.len(), 1);
        assert!(!item.fallthrough);
    }
}
