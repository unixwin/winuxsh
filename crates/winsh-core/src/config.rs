//! Shell configuration types.

use std::collections::HashMap;
use std::path::PathBuf;

/// The command execution backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    /// Use system commands (default PATH lookup)
    System,
    /// Use winuxcmd/uutils coreutils implementation
    WinuxCmd,
    /// Auto-detect: prefer winuxcmd if available, fallback to system
    Auto,
}

impl Default for BackendType {
    fn default() -> Self {
        BackendType::Auto
    }
}

/// Shell configuration.
///
/// This contains all the configuration settings for the shell,
/// including prompt strings, colors, aliases, and plugin settings.
#[derive(Debug, Clone)]
pub struct ShellConfig {
    /// Primary prompt string
    pub prompt: String,
    /// Right prompt string
    pub rprompt: String,
    /// Continuation prompt string
    pub ps2: String,
    /// History file path
    pub history_file: PathBuf,
    /// History size (number of entries to keep in memory)
    pub history_size: usize,
    /// History file size (number of entries to save)
    pub history_save_size: usize,
    /// Configuration file paths
    pub config_files: Vec<PathBuf>,
    /// Plugin directories
    pub plugin_dirs: Vec<PathBuf>,
    /// Theme name
    pub theme: String,
    /// Custom aliases
    pub aliases: HashMap<String, String>,
    /// Custom environment variables
    pub env_vars: HashMap<String, String>,
    /// Command execution backend
    pub backend: BackendType,
    /// Path to winuxcmd binary (when using WinuxCmd backend)
    pub winuxcmd_path: Option<PathBuf>,
    /// Oh-My-Winuxsh directory
    pub oh_my_winsh_dir: Option<PathBuf>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let history_file = home.join(".winsh_history");

        Self {
            prompt: "%n@%m %~ %# ".to_string(),
            rprompt: String::new(),
            ps2: "%_> ".to_string(),
            history_file,
            history_size: 10000,
            history_save_size: 10000,
            config_files: vec![home.join(".winshrc"), home.join(".winshrc.toml")],
            plugin_dirs: vec![home.join(".winsh").join("plugins")],
            theme: "default".to_string(),
            aliases: HashMap::new(),
            env_vars: HashMap::new(),
            backend: BackendType::Auto,
            winuxcmd_path: None,
            oh_my_winsh_dir: Some(home.join(".oh-my-winuxsh")),
        }
    }
}

/// Color configuration for the shell.
#[derive(Debug, Clone)]
pub struct ShellColors {
    /// Prompt color
    pub prompt: String,
    /// Error color
    pub error: String,
    /// Warning color
    pub warning: String,
    /// Success color
    pub success: String,
    /// Info color
    pub info: String,
    /// Directory color
    pub directory: String,
    /// Executable color
    pub executable: String,
    /// Symlink color
    pub symlink: String,
}

impl Default for ShellColors {
    fn default() -> Self {
        Self {
            prompt: "green".to_string(),
            error: "red".to_string(),
            warning: "yellow".to_string(),
            success: "green".to_string(),
            info: "blue".to_string(),
            directory: "blue".to_string(),
            executable: "green".to_string(),
            symlink: "cyan".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_config_default() {
        let config = ShellConfig::default();
        assert!(!config.prompt.is_empty());
        assert_eq!(config.history_size, 10000);
        assert_eq!(config.theme, "default");
    }

    #[test]
    fn test_shell_colors_default() {
        let colors = ShellColors::default();
        assert_eq!(colors.prompt, "green");
        assert_eq!(colors.error, "red");
    }
}
