//! Error types for the WinSH shell.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Result type alias for WinSH operations.
pub type ShellResult<T> = Result<T, ShellError>;

/// Errors that can occur in the WinSH shell.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    // I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    // Parse errors
    #[error("syntax error at line {line}, column {col}: {message}")]
    SyntaxError {
        line: usize,
        col: usize,
        message: String,
    },

    #[error("unexpected token '{token}' at line {line}, column {col}")]
    UnexpectedToken {
        token: String,
        line: usize,
        col: usize,
    },

    #[error("unexpected end of input")]
    UnexpectedEof,

    #[error("unterminated {what} starting at line {line}")]
    Unterminated { what: String, line: usize },

    // Command errors
    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),

    #[error("no such file or directory: {0}")]
    NoSuchFile(PathBuf),

    #[error("is a directory: {0}")]
    IsADirectory(PathBuf),

    // Variable errors
    #[error("unbound variable: {0}")]
    UnboundVariable(String),

    #[error("readonly variable: {0}")]
    ReadonlyVariable(String),

    #[error("invalid variable name: {0}")]
    InvalidVariableName(String),

    // Arithmetic errors
    #[error("division by zero")]
    DivisionByZero,

    #[error("arithmetic syntax error: {0}")]
    ArithmeticSyntax(String),

    // Job control errors
    #[error("no current job")]
    NoCurrentJob,

    #[error("no such job: {0}")]
    NoSuchJob(String),

    #[error("job not found: {0}")]
    JobNotFound(u32),

    #[error("job has already completed")]
    JobCompleted,

    // Function errors
    #[error("function not found: {0}")]
    FunctionNotFound(String),

    #[error("function already exists: {0}")]
    FunctionExists(String),

    // Array errors
    #[error("array index out of bounds: {0}")]
    ArrayIndexOutOfBounds(usize),

    #[error("not an array: {0}")]
    NotAnArray(String),

    // Configuration errors
    #[error("config error: {0}")]
    ConfigError(String),

    #[error("invalid option: {0}")]
    InvalidOption(String),

    // Plugin errors
    #[error("plugin error: {0}")]
    PluginError(String),

    #[error("plugin not found: {0}")]
    PluginNotFound(String),

    // Shell errors
    #[error("shell error: {0}")]
    ShellError(String),

    #[error("interrupted")]
    Interrupted,

    #[error("exit with code {0}")]
    Exit(i32),

    // Redirection errors
    #[error("redirection error: {0}")]
    RedirectionError(String),

    #[error("ambiguous redirect: {0}")]
    AmbiguousRedirect(String),

    // Expansion errors
    #[error("expansion error: {0}")]
    ExpansionError(String),

    #[error("bad substitution: {0}")]
    BadSubstitution(String),

    // History errors
    #[error("history error: {0}")]
    HistoryError(String),

    // Completion errors
    #[error("completion error: {0}")]
    CompletionError(String),

    // FFI errors
    #[error("FFI error: {0}")]
    FfiError(String),

    // Generic errors
    #[error("{0}")]
    Other(String),
}

impl ShellError {
    /// Create a syntax error.
    pub fn syntax(line: usize, col: usize, message: impl Into<String>) -> Self {
        Self::SyntaxError {
            line,
            col,
            message: message.into(),
        }
    }

    /// Create an unexpected token error.
    pub fn unexpected_token(token: impl Into<String>, line: usize, col: usize) -> Self {
        Self::UnexpectedToken {
            token: token.into(),
            line,
            col,
        }
    }

    /// Create an unterminated error.
    pub fn unterminated(what: impl Into<String>, line: usize) -> Self {
        Self::Unterminated {
            what: what.into(),
            line,
        }
    }

    /// Create a command not found error.
    pub fn command_not_found(cmd: impl Into<String>) -> Self {
        Self::CommandNotFound(cmd.into())
    }

    /// Create a permission denied error.
    pub fn permission_denied(path: impl Into<String>) -> Self {
        Self::PermissionDenied(path.into())
    }

    /// Create a shell error with a message.
    pub fn message(msg: impl Into<String>) -> Self {
        Self::ShellError(msg.into())
    }

    /// Create an exit error.
    pub fn exit(code: i32) -> Self {
        Self::Exit(code)
    }

    /// Get the exit code for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Exit(code) => *code,
            Self::CommandNotFound(_) => 127,
            Self::PermissionDenied(_) => 126,
            Self::Interrupted => 130,
            _ => 1,
        }
    }

    /// Check if this error is a fatal error (should terminate the shell).
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Exit(_))
    }
}

/// Convert from anyhow error to ShellError.
impl From<anyhow::Error> for ShellError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ShellError::command_not_found("ls");
        assert_eq!(err.to_string(), "command not found: ls");
    }

    #[test]
    fn test_error_exit_code() {
        let err = ShellError::command_not_found("ls");
        assert_eq!(err.exit_code(), 127);

        let err = ShellError::permission_denied("/usr/bin/ls");
        assert_eq!(err.exit_code(), 126);

        let err = ShellError::Interrupted;
        assert_eq!(err.exit_code(), 130);

        let err = ShellError::exit(42);
        assert_eq!(err.exit_code(), 42);
    }

    #[test]
    fn test_error_is_fatal() {
        assert!(ShellError::exit(0).is_fatal());
        assert!(!ShellError::command_not_found("ls").is_fatal());
    }

    #[test]
    fn test_syntax_error() {
        let err = ShellError::syntax(1, 5, "unexpected token");
        assert_eq!(
            err.to_string(),
            "syntax error at line 1, column 5: unexpected token"
        );
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let shell_err: ShellError = io_err.into();
        assert!(shell_err.to_string().contains("I/O error"));
    }
}
