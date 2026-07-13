//! Theme system for winuxsh
//!
//! Defines prompt / output colours for the shell.

use nu_ansi_term::{Style, Color};

/// A colour theme for the shell.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
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
            name: "default",
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
            name: "dark",
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
            name: "light",
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
            name: "colorful",
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
    match name {
        "dark" => Theme::dark(),
        "light" => Theme::light(),
        "colorful" => Theme::colorful(),
        _ => Theme::default_theme(),
    }
}

/// List all available theme names.
pub fn list_names() -> &'static [&'static str] {
    &["default", "dark", "light", "colorful"]
}


