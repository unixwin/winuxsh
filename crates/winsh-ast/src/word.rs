//! Word types - represents strings with possible expansions.

use std::fmt;

/// A word in a shell command, which may contain expansions.
///
/// A word is a sequence of one or more parts that are concatenated together.
/// Parts can be literal text, variable references, command substitutions, etc.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub parts: Vec<WordPart>,
    pub span: crate::span::Span,
}

/// A part of a word.
#[derive(Debug, Clone, PartialEq)]
pub enum WordPart {
    /// Literal text
    Literal(String),
    /// A simple variable reference: $VAR
    Variable(String),
    /// A braced variable reference: ${VAR}
    BracedVariable(String),
    /// A variable with default value: ${VAR:-default}
    VariableDefault {
        name: String,
        operator: VarOperator,
        value: String,
    },
    /// A variable with pattern removal: ${VAR#pattern}
    VariablePattern {
        name: String,
        operator: PatternOperator,
        pattern: String,
    },
    /// A variable with substitution: ${VAR/old/new}
    VariableSubst {
        name: String,
        old: String,
        new: String,
        all: bool, // true for ${VAR//old/new}
    },
    /// A variable with case modification: ${(u)VAR}
    VariableCase { name: String, case: CaseOperator },
    /// A variable length: ${#VAR}
    VariableLength(String),
    /// A command substitution: $(command)
    CommandSubst(String),
    /// A backtick command substitution: `command`
    BacktickSubst(String),
    /// An arithmetic expansion: $((expr))
    Arithmetic(String),
    /// A glob pattern: *, ?, [...]
    Glob(GlobPattern),
    /// A tilde expansion: ~, ~user
    Tilde(Option<String>),
    /// A single-quoted literal (no expansion)
    SingleQuoted(String),
    /// A double-quoted string (may contain expansions)
    DoubleQuoted(Vec<WordPart>),
    /// A dollar-quoted string with ANSI C escapes
    DollarQuoted(String),
    /// A literal backslash-escaped character
    Escaped(char),
}

/// Variable default value operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarOperator {
    /// ${VAR:-default} - use default if unset or empty
    Minus,
    /// ${VAR:=default} - assign default if unset or empty
    Equals,
    /// ${VAR:+alternate} - use alternate if set and non-empty
    Plus,
    /// ${VAR:?error} - error if unset or empty
    Question,
    /// ${VAR-default} - use default if unset
    ColonMinus,
    /// ${VAR=default} - assign default if unset
    ColonEquals,
    /// ${VAR+alternate} - use alternate if set
    ColonPlus,
    /// ${VAR?error} - error if unset
    ColonQuestion,
}

/// Pattern removal operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternOperator {
    /// ${VAR#pattern} - remove shortest matching prefix
    Hash,
    /// ${VAR##pattern} - remove longest matching prefix
    DoubleHash,
    /// ${VAR%pattern} - remove shortest matching suffix
    Percent,
    /// ${VAR%%pattern} - remove longest matching suffix
    DoublePercent,
}

/// Case modification operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseOperator {
    /// ${(u)VAR} - uppercase
    Upper,
    /// ${(l)VAR} - lowercase
    Lower,
    /// ${(C)VAR} - capitalize first letter
    Capitalize,
    /// ${(U)VAR} - uppercase all
    UpperAll,
    /// ${(L)VAR} - lowercase all
    LowerAll,
}

/// A glob pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobPattern {
    /// The pattern string
    pub pattern: String,
    /// Whether this is a recursive glob (**)
    pub recursive: bool,
}

impl Word {
    /// Create a simple word with a single literal part.
    pub fn literal(s: impl Into<String>) -> Self {
        let s = s.into();
        Self {
            span: crate::span::Span::empty(),
            parts: vec![WordPart::Literal(s)],
        }
    }

    /// Create a variable reference word.
    pub fn variable(name: impl Into<String>) -> Self {
        Self {
            span: crate::span::Span::empty(),
            parts: vec![WordPart::Variable(name.into())],
        }
    }

    /// Check if this word is a simple literal (no expansions).
    pub fn is_literal(&self) -> bool {
        self.parts.iter().all(|p| matches!(p, WordPart::Literal(_)))
    }

    /// Get the literal value if this is a simple literal word.
    pub fn as_literal(&self) -> Option<&str> {
        if self.parts.len() == 1 {
            if let WordPart::Literal(s) = &self.parts[0] {
                return Some(s);
            }
        }
        None
    }

    /// Check if this word contains any glob patterns.
    pub fn has_glob(&self) -> bool {
        self.parts.iter().any(|p| matches!(p, WordPart::Glob(_)))
    }

    /// Check if this word contains any variable expansions.
    pub fn has_variable(&self) -> bool {
        self.parts.iter().any(|p| {
            matches!(
                p,
                WordPart::Variable(_)
                    | WordPart::BracedVariable(_)
                    | WordPart::VariableDefault { .. }
                    | WordPart::VariablePattern { .. }
                    | WordPart::VariableSubst { .. }
                    | WordPart::VariableCase { .. }
                    | WordPart::VariableLength(_)
            )
        })
    }

    /// Check if this word contains any command substitutions.
    pub fn has_command_subst(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, WordPart::CommandSubst(_) | WordPart::BacktickSubst(_)))
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in &self.parts {
            write!(f, "{}", part)?;
        }
        Ok(())
    }
}

impl fmt::Display for WordPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WordPart::Literal(s) => write!(f, "{}", s),
            WordPart::Variable(name) => write!(f, "${}", name),
            WordPart::BracedVariable(name) => write!(f, "${{{}}}", name),
            WordPart::VariableDefault {
                name,
                operator,
                value,
            } => {
                let op = match operator {
                    VarOperator::Minus => ":-",
                    VarOperator::Equals => ":=",
                    VarOperator::Plus => ":+",
                    VarOperator::Question => ":?",
                    VarOperator::ColonMinus => "-",
                    VarOperator::ColonEquals => "=",
                    VarOperator::ColonPlus => "+",
                    VarOperator::ColonQuestion => "?",
                };
                write!(f, "${{{}{}{}}}", name, op, value)
            }
            WordPart::VariablePattern {
                name,
                operator,
                pattern,
            } => {
                let op = match operator {
                    PatternOperator::Hash => "#",
                    PatternOperator::DoubleHash => "##",
                    PatternOperator::Percent => "%",
                    PatternOperator::DoublePercent => "%%",
                };
                write!(f, "${{{}{}{}}}", name, op, pattern)
            }
            WordPart::VariableSubst {
                name,
                old,
                new,
                all,
            } => {
                let sep = if *all { "//" } else { "/" };
                write!(f, "${{{}{}{}/{}}}", name, sep, old, new)
            }
            WordPart::VariableCase { name, case } => {
                let op = match case {
                    CaseOperator::Upper => "u",
                    CaseOperator::Lower => "l",
                    CaseOperator::Capitalize => "C",
                    CaseOperator::UpperAll => "U",
                    CaseOperator::LowerAll => "L",
                };
                write!(f, "${{{}({}){}}}", op, name, "")
            }
            WordPart::VariableLength(name) => write!(f, "${{{}#}}", name),
            WordPart::CommandSubst(cmd) => write!(f, "$({})", cmd),
            WordPart::BacktickSubst(cmd) => write!(f, "`{}`", cmd),
            WordPart::Arithmetic(expr) => write!(f, "$(({}))", expr),
            WordPart::Glob(glob) => write!(f, "{}", glob.pattern),
            WordPart::Tilde(user) => {
                if let Some(u) = user {
                    write!(f, "~{}", u)
                } else {
                    write!(f, "~")
                }
            }
            WordPart::SingleQuoted(s) => write!(f, "'{}'", s),
            WordPart::DoubleQuoted(parts) => {
                write!(f, "\"")?;
                for p in parts {
                    write!(f, "{}", p)?;
                }
                write!(f, "\"")
            }
            WordPart::DollarQuoted(s) => write!(f, "$'{}'", s),
            WordPart::Escaped(c) => write!(f, "\\{}", c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_literal() {
        let word = Word::literal("hello");
        assert!(word.is_literal());
        assert_eq!(word.as_literal(), Some("hello"));
    }

    #[test]
    fn test_word_variable() {
        let word = Word::variable("HOME");
        assert!(!word.is_literal());
        assert!(word.has_variable());
        assert!(!word.has_glob());
    }

    #[test]
    fn test_word_display() {
        let word = Word::literal("hello");
        assert_eq!(word.to_string(), "hello");

        let word = Word::variable("HOME");
        assert_eq!(word.to_string(), "$HOME");
    }

    #[test]
    fn test_word_has_glob() {
        let word = Word {
            span: crate::span::Span::empty(),
            parts: vec![WordPart::Glob(GlobPattern {
                pattern: "*.txt".to_string(),
                recursive: false,
            })],
        };
        assert!(word.has_glob());
    }
}
