//! Redirection types.

use crate::word::Word;
use std::fmt;

/// A redirection in a command.
#[derive(Debug, Clone, PartialEq)]
pub struct Redirection {
    /// The file descriptor to redirect (if specified, e.g., 2>)
    pub fd: Option<u32>,
    /// The redirection operator
    pub op: RedirOp,
    /// The target of the redirection
    pub target: RedirTarget,
}

/// Redirection operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirOp {
    /// Input redirect: <
    In,
    /// Output redirect: >
    Out,
    /// Append redirect: >>
    Append,
    /// Stderr redirect: 2>
    Err,
    /// Stderr append: 2>>
    ErrAppend,
    /// Stderr to stdout: 2>&1
    ErrToOut,
    /// Stdout to stderr: 1>&2
    OutToErr,
    /// Combined redirect: &>
    Combined,
    /// Combined append: &>>
    CombinedAppend,
    /// Here document: <<
    HereDoc,
    /// Here string: <<<
    HereString,
    /// Duplicate input descriptor: <&N
    DupIn,
    /// Duplicate output descriptor: >&N
    DupOut,
    /// Close file descriptor: <&-
    CloseIn,
    /// Close file descriptor: >&-
    CloseOut,
}

/// The target of a redirection.
#[derive(Debug, Clone, PartialEq)]
pub enum RedirTarget {
    /// A file path
    File(Word),
    /// A file descriptor number
    Fd(u32),
    /// Close the descriptor
    Close,
    /// Here document content
    HereDoc {
        delimiter: String,
        content: String,
        strip_tabs: bool,
    },
    /// Here string content
    HereString(Word),
}

impl Redirection {
    /// Create a simple input redirection.
    pub fn input(file: Word) -> Self {
        Self {
            fd: None,
            op: RedirOp::In,
            target: RedirTarget::File(file),
        }
    }

    /// Create a simple output redirection.
    pub fn output(file: Word) -> Self {
        Self {
            fd: None,
            op: RedirOp::Out,
            target: RedirTarget::File(file),
        }
    }

    /// Create an append redirection.
    pub fn append(file: Word) -> Self {
        Self {
            fd: None,
            op: RedirOp::Append,
            target: RedirTarget::File(file),
        }
    }

    /// Create a stderr redirection.
    pub fn stderr(file: Word) -> Self {
        Self {
            fd: Some(2),
            op: RedirOp::Err,
            target: RedirTarget::File(file),
        }
    }

    /// Create a stderr-to-stdout redirection.
    pub fn stderr_to_stdout() -> Self {
        Self {
            fd: Some(2),
            op: RedirOp::ErrToOut,
            target: RedirTarget::Fd(1),
        }
    }

    /// Create a stdout-to-stderr redirection.
    pub fn stdout_to_stderr() -> Self {
        Self {
            fd: Some(1),
            op: RedirOp::OutToErr,
            target: RedirTarget::Fd(2),
        }
    }
}

impl fmt::Display for RedirOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedirOp::In => write!(f, "<"),
            RedirOp::Out => write!(f, ">"),
            RedirOp::Append => write!(f, ">>"),
            RedirOp::Err => write!(f, "2>"),
            RedirOp::ErrAppend => write!(f, "2>>"),
            RedirOp::ErrToOut => write!(f, "2>&1"),
            RedirOp::OutToErr => write!(f, "1>&2"),
            RedirOp::Combined => write!(f, "&>"),
            RedirOp::CombinedAppend => write!(f, "&>>"),
            RedirOp::HereDoc => write!(f, "<<"),
            RedirOp::HereString => write!(f, "<<<"),
            RedirOp::DupIn => write!(f, "<&"),
            RedirOp::DupOut => write!(f, ">&"),
            RedirOp::CloseIn => write!(f, "<&-"),
            RedirOp::CloseOut => write!(f, ">&-"),
        }
    }
}

impl fmt::Display for Redirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For some redirections, the fd is already part of the operator
        match self.op {
            RedirOp::ErrToOut | RedirOp::OutToErr => {
                // These operators already include the full specification (2>&1, 1>&2)
                write!(f, "{}", self.op)
            }
            RedirOp::Combined | RedirOp::CombinedAppend => {
                // These operators don't need the fd prefix
                write!(f, "{}{}", self.op, self.target)
            }
            _ => {
                if let Some(fd) = self.fd {
                    write!(f, "{}", fd)?;
                }
                write!(f, "{}{}", self.op, self.target)
            }
        }
    }
}

impl fmt::Display for RedirTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedirTarget::File(word) => write!(f, "{}", word),
            RedirTarget::Fd(fd) => write!(f, "&{}", fd),
            RedirTarget::Close => write!(f, "&-"),
            RedirTarget::HereDoc { delimiter, .. } => write!(f, "{}", delimiter),
            RedirTarget::HereString(word) => write!(f, "{}", word),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::word::Word;

    #[test]
    fn test_redirection_input() {
        let redir = Redirection::input(Word::literal("file.txt"));
        assert_eq!(redir.op, RedirOp::In);
        assert_eq!(redir.fd, None);
    }

    #[test]
    fn test_redirection_output() {
        let redir = Redirection::output(Word::literal("file.txt"));
        assert_eq!(redir.op, RedirOp::Out);
    }

    #[test]
    fn test_redirection_stderr() {
        let redir = Redirection::stderr(Word::literal("err.log"));
        assert_eq!(redir.op, RedirOp::Err);
        assert_eq!(redir.fd, Some(2));
    }

    #[test]
    fn test_redirection_display() {
        let redir = Redirection::output(Word::literal("file.txt"));
        assert_eq!(redir.to_string(), ">file.txt");

        let redir = Redirection::stderr_to_stdout();
        assert_eq!(redir.to_string(), "2>&1");
    }
}
