//! Prompt rendering for winuxsh
//!
//! Implements the `reedline::Prompt` trait using a template string
//! with substitutions: {user}, {host}, {cwd}, {symbol}.

use std::borrow::Cow;

use crate::theme::{by_name, Theme};
use reedline::{Prompt, PromptEditMode, PromptHistorySearch};

/// A prompt that renders the configured template with theme-aware ANSI colours.
pub struct WinuxshPrompt {
    template: String,
    right_template: Option<String>,
    theme: Theme,
}

impl WinuxshPrompt {
    pub fn new(template: Option<String>, right_template: Option<String>, theme_name: &str) -> Self {
        let t = template.unwrap_or_else(|| "{user}@{host} {cwd} %# ".to_string());
        Self {
            template: t,
            right_template,
            theme: by_name(theme_name),
        }
    }

    pub fn set_theme(&mut self, theme_name: &str) {
        self.theme = by_name(theme_name);
    }

    fn render_template(&self, template: &str) -> String {
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "?".to_string());
        let host = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "winhost".to_string());
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "?".to_string());

        let user_s = self.theme.prompt_user.paint(&user).to_string();
        let host_s = self.theme.prompt_host.paint(&host).to_string();
        let dir_s = self.theme.prompt_dir.paint(&cwd).to_string();
        let sym_s = self.theme.prompt_symbol.paint("%").to_string();

        template
            .replace("{user}", &user_s)
            .replace("{host}", &host_s)
            .replace("{cwd}", &dir_s)
            .replace("{symbol}", &sym_s)
            .replace("%#", &sym_s)
            .replace("%n", &user)
            .replace("%m", &host)
            .replace("%~", &cwd)
    }
}

impl Prompt for WinuxshPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Owned(self.render_template(&self.template))
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        match &self.right_template {
            Some(template) => Cow::Owned(self.render_template(template)),
            None => Cow::Borrowed(""),
        }
    }

    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        _search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Borrowed("(history search) ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_optional_right_prompt() {
        let prompt = WinuxshPrompt::new(
            Some("left> ".to_string()),
            Some("right".to_string()),
            "default",
        );

        assert_eq!(prompt.render_prompt_right(), "right");
    }

    #[test]
    fn omits_right_prompt_when_unset() {
        let prompt = WinuxshPrompt::new(Some("left> ".to_string()), None, "default");

        assert_eq!(prompt.render_prompt_right(), "");
    }
}
