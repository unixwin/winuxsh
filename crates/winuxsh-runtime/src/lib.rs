//! winuxsh-runtime: Windows bash-compatible shell runtime
//!
//! Built on top of rubash (shell language engine) and winuxcmd (coreutils).
//! This crate provides the interactive shell experience: reedline REPL,
//! completion system, theming, configuration, and Windows integration.

pub mod autosuggest;
pub mod completion;
pub mod config;
pub mod ctrl_c;
pub mod prompt;
pub mod repl;
pub mod shell;
pub mod syntax_highlighting;
pub mod theme;
pub mod winuxcmd;
pub mod zsh_compat;

pub use shell::Shell;
pub use config::{
    AutosuggestConfig, EditorConfig, EditorMode, ShellConfig, SyntaxHighlightConfig,
    ZshCompatLevel, ZshConfig,
};
pub use theme::Theme;
pub use completion::WinuxshCompleter;
pub use completion::CompletionState;
