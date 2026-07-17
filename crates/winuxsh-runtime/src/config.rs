//! Configuration loading from `.winshrc.toml`

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

/// Shell configuration, loaded from `~/.winshrc.toml`.
#[derive(Debug, Clone, Default)]
pub struct ShellConfig {
    /// Prompt template (e.g. "{user}@{host} {cwd} {symbol}")
    pub prompt_format: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Emacs,
    Vi,
}

impl Default for EditorMode {
    fn default() -> Self {
        Self::Emacs
    }
}

impl EditorMode {
    fn from_config_value(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "emacs" => Self::Emacs,
            "vi" => Self::Vi,
            other => {
                log::warn!("Unknown editor edit_mode '{}', falling back to emacs", other);
                Self::Emacs
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EditorConfig {
    pub edit_mode: EditorMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZshCompatLevel {
    Safe,
    Warn,
    Experimental,
}

impl Default for ZshCompatLevel {
    fn default() -> Self {
        Self::Safe
    }
}

impl ZshCompatLevel {
    fn from_config_value(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "safe" => Self::Safe,
            "warn" => Self::Warn,
            "experimental" => Self::Experimental,
            other => {
                log::warn!(
                    "Unknown zsh compat_level '{}', falling back to safe",
                    other
                );
                Self::Safe
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ZshConfig {
    pub enabled: bool,
    pub zdotdir: Option<PathBuf>,
    pub import_zshrc: bool,
    pub import_oh_my_zsh: bool,
    pub plugins: Vec<String>,
    pub compat_level: ZshCompatLevel,
    pub auto_apply: bool,
    pub autosuggestions: AutosuggestConfig,
}

impl Default for ZshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            zdotdir: None,
            import_zshrc: true,
            import_oh_my_zsh: true,
            plugins: Vec::new(),
            compat_level: ZshCompatLevel::Safe,
            auto_apply: false,
            autosuggestions: AutosuggestConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutosuggestConfig {
    pub enabled: bool,
    pub strategies: Vec<String>,
    pub highlight_style: String,
    pub buffer_max_size: Option<usize>,
}

impl Default for AutosuggestConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strategies: vec!["history".to_string()],
            highlight_style: "fg=8".to_string(),
            buffer_max_size: None,
        }
    }
}

impl AutosuggestConfig {
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(value) = std::env::var("ZSH_AUTOSUGGEST_STRATEGY") {
            let strategies = parse_autosuggest_strategy_value(&value);
            if !strategies.is_empty() {
                self.strategies = strategies;
            }
        }
        if let Ok(value) = std::env::var("ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE") {
            if !value.trim().is_empty() {
                self.highlight_style = value;
            }
        }
        if let Ok(value) = std::env::var("ZSH_AUTOSUGGEST_BUFFER_MAX_SIZE") {
            match value.trim().parse::<usize>() {
                Ok(max_size) => self.buffer_max_size = Some(max_size),
                Err(err) => log::warn!(
                    "Invalid ZSH_AUTOSUGGEST_BUFFER_MAX_SIZE '{}': {}",
                    value,
                    err
                ),
            }
        }
        self
    }

    pub fn history_strategy_enabled(&self) -> bool {
        self.enabled
            && self
                .strategies
                .iter()
                .any(|strategy| strategy.eq_ignore_ascii_case("history"))
    }
}

fn parse_autosuggest_strategy_value(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.trim_matches('"').trim_matches('\'').to_ascii_lowercase())
        .collect()
}

/// Top-level TOML structure.
#[derive(Debug, Deserialize)]
struct WinshrcToml {
    shell: Option<ShellToml>,
    theme: Option<ThemeToml>,
    editor: Option<EditorToml>,
    aliases: Option<HashMap<String, String>>,
    completions: Option<CompletionsToml>,
    winuxcmd: Option<WinuxCmdToml>,
    zsh: Option<ZshToml>,
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
struct EditorToml {
    edit_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompletionsToml {
    completion_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WinuxCmdToml {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZshToml {
    enabled: Option<bool>,
    zdotdir: Option<String>,
    import_zshrc: Option<bool>,
    import_oh_my_zsh: Option<bool>,
    plugins: Option<Vec<String>>,
    compat_level: Option<String>,
    auto_apply: Option<bool>,
    autosuggestions: Option<AutosuggestToml>,
}

#[derive(Debug, Deserialize)]
struct AutosuggestToml {
    enabled: Option<bool>,
    strategy: Option<Vec<String>>,
    highlight_style: Option<String>,
    buffer_max_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct FullConfig {
    pub shell: ShellConfig,
    pub editor: EditorConfig,
    pub theme_name: String,
    pub aliases: HashMap<String, String>,
    pub completion_dirs: Vec<PathBuf>,
    pub winuxcmd_path: Option<PathBuf>,
    pub zsh: ZshConfig,
}

impl Default for FullConfig {
    fn default() -> Self {
        Self {
            shell: ShellConfig::default(),
            editor: EditorConfig::default(),
            theme_name: "default".to_string(),
            aliases: HashMap::new(),
            completion_dirs: Vec::new(),
            winuxcmd_path: None,
            zsh: ZshConfig::default(),
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

    build_config(parsed)
}

fn build_config(parsed: WinshrcToml) -> FullConfig {
    let zsh = parsed.zsh.map(build_zsh_config).unwrap_or_default();

    FullConfig {
        shell: ShellConfig {
            prompt_format: parsed.shell.and_then(|s| s.prompt_format),
        },
        editor: EditorConfig {
            edit_mode: parsed
                .editor
                .and_then(|e| e.edit_mode)
                .map(|mode| EditorMode::from_config_value(&mode))
                .unwrap_or_default(),
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
        zsh,
    }
}

fn build_zsh_config(parsed: ZshToml) -> ZshConfig {
    ZshConfig {
        enabled: parsed.enabled.unwrap_or(false),
        zdotdir: parsed.zdotdir.map(PathBuf::from),
        import_zshrc: parsed.import_zshrc.unwrap_or(true),
        import_oh_my_zsh: parsed.import_oh_my_zsh.unwrap_or(true),
        plugins: parsed.plugins.unwrap_or_default(),
        compat_level: parsed
            .compat_level
            .map(|level| ZshCompatLevel::from_config_value(&level))
            .unwrap_or_default(),
        auto_apply: parsed.auto_apply.unwrap_or(false),
        autosuggestions: parsed
            .autosuggestions
            .map(build_autosuggest_config)
            .unwrap_or_default(),
    }
}

fn build_autosuggest_config(parsed: AutosuggestToml) -> AutosuggestConfig {
    let defaults = AutosuggestConfig::default();
    AutosuggestConfig {
        enabled: parsed.enabled.unwrap_or(defaults.enabled),
        strategies: parsed.strategy.unwrap_or(defaults.strategies),
        highlight_style: parsed.highlight_style.unwrap_or(defaults.highlight_style),
        buffer_max_size: parsed.buffer_max_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(input: &str) -> FullConfig {
        build_config(toml::from_str(input).unwrap())
    }

    #[test]
    fn defaults_to_emacs_edit_mode() {
        let config = parse_config("");
        assert_eq!(config.editor.edit_mode, EditorMode::Emacs);
    }

    #[test]
    fn parses_vi_edit_mode() {
        let config = parse_config(
            r#"
[editor]
edit_mode = "vi"
"#,
        );
        assert_eq!(config.editor.edit_mode, EditorMode::Vi);
    }

    #[test]
    fn unknown_edit_mode_falls_back_to_emacs() {
        let config = parse_config(
            r#"
[editor]
edit_mode = "unknown"
"#,
        );
        assert_eq!(config.editor.edit_mode, EditorMode::Emacs);
    }

    #[test]
    fn parses_zsh_config() {
        let config = parse_config(
            r#"
[zsh]
enabled = true
zdotdir = "C:/Users/me"
import_zshrc = false
import_oh_my_zsh = true
plugins = ["git", "zsh-autosuggestions"]
compat_level = "warn"
auto_apply = true

[zsh.autosuggestions]
enabled = true
strategy = ["history", "completion"]
highlight_style = "fg=#ff00ff,bg=cyan,bold,underline"
buffer_max_size = 20
"#,
        );

        assert!(config.zsh.enabled);
        assert_eq!(config.zsh.zdotdir, Some(PathBuf::from("C:/Users/me")));
        assert!(!config.zsh.import_zshrc);
        assert!(config.zsh.import_oh_my_zsh);
        assert_eq!(config.zsh.plugins, vec!["git", "zsh-autosuggestions"]);
        assert_eq!(config.zsh.compat_level, ZshCompatLevel::Warn);
        assert!(config.zsh.auto_apply);
        assert!(config.zsh.autosuggestions.enabled);
        assert_eq!(
            config.zsh.autosuggestions.strategies,
            vec!["history", "completion"]
        );
        assert_eq!(
            config.zsh.autosuggestions.highlight_style,
            "fg=#ff00ff,bg=cyan,bold,underline"
        );
        assert_eq!(config.zsh.autosuggestions.buffer_max_size, Some(20));
    }

    #[test]
    fn parses_zsh_autosuggest_strategy_env_style() {
        assert_eq!(
            parse_autosuggest_strategy_value("(history completion)"),
            vec!["history", "completion"]
        );
        assert_eq!(
            parse_autosuggest_strategy_value("history,match_prev_cmd"),
            vec!["history", "match_prev_cmd"]
        );
    }

    #[test]
    fn history_strategy_requires_enabled_history() {
        let mut config = AutosuggestConfig::default();
        assert!(config.history_strategy_enabled());

        config.enabled = false;
        assert!(!config.history_strategy_enabled());

        config.enabled = true;
        config.strategies = vec!["completion".to_string()];
        assert!(!config.history_strategy_enabled());
    }
}
