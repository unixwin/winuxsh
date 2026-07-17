//! Theme system for winuxsh
//!
//! Defines prompt / output colours for the shell.

use std::path::{Path, PathBuf};

use nu_ansi_term::{Color, Style};
use serde::Deserialize;

/// A colour theme for the shell.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Theme {
    pub name: String,
    pub prompt_user: Style,
    pub prompt_host: Style,
    pub prompt_dir: Style,
    pub prompt_symbol: Style,
    pub error: Style,
    pub warning: Style,
    pub success: Style,
}

impl Theme {
    pub fn default_theme() -> Self {
        Self {
            name: "default".to_string(),
            prompt_user: Style::new().bold().fg(Color::Green),
            prompt_host: Style::new().bold().fg(Color::Cyan),
            prompt_dir: Style::new().bold().fg(Color::Blue),
            prompt_symbol: Style::new().fg(Color::White),
            error: Style::new().fg(Color::Red),
            warning: Style::new().fg(Color::Yellow),
            success: Style::new().fg(Color::Green),
        }
    }

    pub fn dark() -> Self {
        Self {
            name: "dark".to_string(),
            prompt_user: Style::new().bold().fg(Color::White),
            prompt_host: Style::new().bold().fg(Color::White),
            prompt_dir: Style::new().bold().fg(Color::LightCyan),
            prompt_symbol: Style::new().fg(Color::White),
            error: Style::new().fg(Color::Red),
            warning: Style::new().fg(Color::Yellow),
            success: Style::new().fg(Color::Green),
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light".to_string(),
            prompt_user: Style::new().bold().fg(Color::Black),
            prompt_host: Style::new().bold().fg(Color::DarkGray),
            prompt_dir: Style::new().bold().fg(Color::Blue),
            prompt_symbol: Style::new().fg(Color::Black),
            error: Style::new().fg(Color::Red),
            warning: Style::new().fg(Color::Yellow),
            success: Style::new().fg(Color::Green),
        }
    }

    pub fn colorful() -> Self {
        Self {
            name: "colorful".to_string(),
            prompt_user: Style::new().bold().fg(Color::LightRed),
            prompt_host: Style::new().bold().fg(Color::LightYellow),
            prompt_dir: Style::new().bold().fg(Color::LightGreen),
            prompt_symbol: Style::new().bold().fg(Color::LightMagenta),
            error: Style::new().bold().fg(Color::Red),
            warning: Style::new().bold().fg(Color::Yellow),
            success: Style::new().bold().fg(Color::Green),
        }
    }
}

/// Look up a theme by name.
pub fn by_name(name: &str) -> Theme {
    if let Some(theme) = builtin_by_name(name) {
        return theme;
    }

    load_user_theme(name).unwrap_or_else(|| {
        log::warn!("Theme '{}' not found, falling back to default", name);
        Theme::default_theme()
    })
}

fn builtin_by_name(name: &str) -> Option<Theme> {
    match name {
        "default" => Some(Theme::default_theme()),
        "dark" => Some(Theme::dark()),
        "light" => Some(Theme::light()),
        "colorful" => Some(Theme::colorful()),
        _ => None,
    }
}

/// List all available theme names.
pub fn list_names() -> &'static [&'static str] {
    &["default", "dark", "light", "colorful"]
}

fn load_user_theme(name: &str) -> Option<Theme> {
    let theme_dir = user_theme_dir()?;
    load_user_theme_from_dir(name, &theme_dir)
}

fn user_theme_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".winuxsh").join("themes"))
}

fn load_user_theme_from_dir(name: &str, theme_dir: &Path) -> Option<Theme> {
    if !is_safe_theme_name(name) {
        log::warn!("Ignoring unsafe theme name '{}'", name);
        return None;
    }

    let path = theme_dir.join(format!("{}.toml", name));
    if !path.is_file() {
        return None;
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) => {
            log::warn!("Failed to read theme {}: {}", path.display(), e);
            return None;
        }
    };

    let parsed: UserThemeToml = match toml::from_str(&content) {
        Ok(parsed) => parsed,
        Err(e) => {
            log::warn!("Failed to parse theme {}: {}", path.display(), e);
            return None;
        }
    };

    parsed.into_theme(name).or_else(|| {
        log::warn!("Failed to build theme {}", path.display());
        None
    })
}

fn is_safe_theme_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

#[derive(Debug, Deserialize)]
struct UserThemeToml {
    prompt_user: Option<UserStyleToml>,
    prompt_host: Option<UserStyleToml>,
    prompt_dir: Option<UserStyleToml>,
    prompt_symbol: Option<UserStyleToml>,
    error: Option<UserStyleToml>,
    warning: Option<UserStyleToml>,
    success: Option<UserStyleToml>,
}

impl UserThemeToml {
    fn into_theme(self, name: &str) -> Option<Theme> {
        let mut theme = Theme::default_theme();
        theme.name = name.to_string();

        if let Some(style) = self.prompt_user {
            theme.prompt_user = style.apply_to(theme.prompt_user)?;
        }
        if let Some(style) = self.prompt_host {
            theme.prompt_host = style.apply_to(theme.prompt_host)?;
        }
        if let Some(style) = self.prompt_dir {
            theme.prompt_dir = style.apply_to(theme.prompt_dir)?;
        }
        if let Some(style) = self.prompt_symbol {
            theme.prompt_symbol = style.apply_to(theme.prompt_symbol)?;
        }
        if let Some(style) = self.error {
            theme.error = style.apply_to(theme.error)?;
        }
        if let Some(style) = self.warning {
            theme.warning = style.apply_to(theme.warning)?;
        }
        if let Some(style) = self.success {
            theme.success = style.apply_to(theme.success)?;
        }

        Some(theme)
    }
}

#[derive(Debug, Deserialize)]
struct UserStyleToml {
    fg: Option<String>,
    bold: Option<bool>,
}

impl UserStyleToml {
    fn apply_to(self, mut style: Style) -> Option<Style> {
        if let Some(fg) = self.fg {
            style = style.fg(parse_color(&fg)?);
        }
        if let Some(bold) = self.bold {
            style.is_bold = bold;
        }
        Some(style)
    }
}

fn parse_color(value: &str) -> Option<Color> {
    let key = value
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-' && !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();

    match key.as_str() {
        "black" => Some(Color::Black),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "red" => Some(Color::Red),
        "lightred" => Some(Color::LightRed),
        "green" => Some(Color::Green),
        "lightgreen" => Some(Color::LightGreen),
        "yellow" => Some(Color::Yellow),
        "lightyellow" => Some(Color::LightYellow),
        "blue" => Some(Color::Blue),
        "lightblue" => Some(Color::LightBlue),
        "purple" => Some(Color::Purple),
        "lightpurple" => Some(Color::LightPurple),
        "magenta" => Some(Color::Magenta),
        "lightmagenta" => Some(Color::LightMagenta),
        "cyan" => Some(Color::Cyan),
        "lightcyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        "lightgray" | "lightgrey" => Some(Color::LightGray),
        _ => {
            log::warn!("Unknown theme color '{}'", value);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn builtin_theme_names_stay_stable() {
        assert_eq!(list_names(), &["default", "dark", "light", "colorful"]);
    }

    #[test]
    fn builtins_take_precedence_over_user_theme_files() {
        let dir = unique_temp_dir("winuxsh-theme-builtins");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("dark.toml"),
            r#"
[prompt_user]
fg = "red"
bold = false
"#,
        )
        .unwrap();

        let builtin = Theme::dark();
        assert_eq!(builtin_by_name("dark"), Some(builtin));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_user_theme_from_theme_dir() {
        let dir = unique_temp_dir("winuxsh-theme-load");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ocean.toml"),
            r#"
[prompt_user]
fg = "light cyan"
bold = false

[prompt_symbol]
fg = "light-magenta"
bold = true

[error]
fg = "red"
bold = true
"#,
        )
        .unwrap();

        let theme = load_user_theme_from_dir("ocean", &dir).unwrap();
        assert_eq!(theme.name, "ocean");
        assert_eq!(theme.prompt_user.foreground, Some(Color::LightCyan));
        assert!(!theme.prompt_user.is_bold);
        assert_eq!(theme.prompt_symbol.foreground, Some(Color::LightMagenta));
        assert!(theme.prompt_symbol.is_bold);
        assert_eq!(theme.error.foreground, Some(Color::Red));
        assert!(theme.error.is_bold);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_user_theme_color_is_ignored() {
        let dir = unique_temp_dir("winuxsh-theme-invalid");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("broken.toml"),
            r#"
[prompt_user]
fg = "not-a-color"
"#,
        )
        .unwrap();

        assert!(load_user_theme_from_dir("broken", &dir).is_none());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unsafe_theme_names_are_ignored() {
        let dir = unique_temp_dir("winuxsh-theme-unsafe");
        std::fs::create_dir_all(&dir).unwrap();

        assert!(load_user_theme_from_dir("../dark", &dir).is_none());
        assert!(load_user_theme_from_dir("", &dir).is_none());

        let _ = std::fs::remove_dir_all(dir);
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
    }
}

