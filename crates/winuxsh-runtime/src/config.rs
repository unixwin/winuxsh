//! Configuration loading from `.winshrc.toml`

use std::collections::HashMap;
use std::path::PathBuf;
use serde::Deserialize;

/// Shell configuration, loaded from `~/.winshrc.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShellConfig {
    /// Prompt template (e.g. "{user}@{host} {cwd} {symbol}")
    pub prompt_format: Option<String>,
}

/// Top-level TOML structure.
#[derive(Debug, Deserialize)]
struct WinshrcToml {
    shell: Option<ShellToml>,
    theme: Option<ThemeToml>,
    aliases: Option<HashMap<String, String>>,
    completions: Option<CompletionsToml>,
    winuxcmd: Option<WinuxCmdToml>,
}

#[derive(Debug, Deserialize)]
struct ShellToml {
    prompt_format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ThemeToml {
    current_theme: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompletionsToml {
    completion_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WinuxCmdToml {
    path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FullConfig {
    pub shell: ShellConfig,
    pub theme_name: String,
    pub aliases: HashMap<String, String>,
    pub completion_dirs: Vec<PathBuf>,
    pub winuxcmd_path: Option<PathBuf>,
}

impl Default for FullConfig {
    fn default() -> Self {
        Self {
            shell: ShellConfig::default(),
            theme_name: "default".to_string(),
            aliases: HashMap::new(),
            completion_dirs: Vec::new(),
            winuxcmd_path: None,
        }
    }
}

/// Load config from `~/.winshrc.toml`. Returns defaults if the file
/// does not exist or cannot be parsed (logs warning).
pub fn load() -> FullConfig {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let config_path = home.join(".winshrc.toml");

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return FullConfig::default(),
    };

    let parsed: WinshrcToml = match toml::from_str(&content) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("Failed to parse {}: {}", config_path.display(), e);
            return FullConfig::default();
        }
    };

    FullConfig {
        shell: ShellConfig {
            prompt_format: parsed.shell.and_then(|s| s.prompt_format),
        },
        theme_name: parsed
            .theme
            .and_then(|t| t.current_theme)
            .unwrap_or_else(|| "default".to_string()),
        aliases: parsed.aliases.unwrap_or_default(),
        completion_dirs: parsed
            .completions
            .and_then(|c| c.completion_dirs)
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        winuxcmd_path: parsed.winuxcmd.and_then(|w| w.path).map(PathBuf::from),
    }
}
