//! # WinSH Core
//!
//! Core types, state management, and error definitions for the WinSH shell.

pub mod arithmetic;
pub mod conditional;
pub mod config;
pub mod env;
pub mod error;
pub mod expansion;
pub mod glob;
pub mod heredoc;
pub mod state;
pub mod value;

pub use arithmetic::eval_arithmetic;
pub use conditional::eval_conditional;
pub use config::{BackendType, ShellConfig};
pub use env::Env;
pub use error::ShellError;
pub use expansion::expand_variable;
pub use glob::{expand_globs, match_pattern, GlobOptions};
pub use heredoc::{parse_heredocs, read_heredoc, HereDoc};
pub use state::ShellState;
pub use value::Value;
