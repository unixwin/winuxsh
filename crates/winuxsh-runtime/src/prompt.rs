//! Prompt rendering for winuxsh
//!
//! Implements the `reedline::Prompt` trait using a template string
//! with substitutions: {user}, {host}, {cwd}, {symbol}.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::theme::{by_name, Theme};
use reedline::{Prompt, PromptEditMode, PromptHistorySearch};

/// A prompt that renders the configured template with theme-aware ANSI colours.
pub struct WinuxshPrompt {
    template: String,
    right_template: Option<String>,
    git_prompt_format: Option<String>,
    theme: Theme,
}

impl WinuxshPrompt {
    pub fn new(
        template: Option<String>,
        right_template: Option<String>,
        git_prompt_format: Option<String>,
        theme_name: &str,
    ) -> Self {
        let t = template.unwrap_or_else(|| "{user}@{host} {cwd} %# ".to_string());
        Self {
            template: t,
            right_template,
            git_prompt_format,
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
        let git_branch = current_git_branch().unwrap_or_default();
        let git_branch_s = if git_branch.is_empty() {
            String::new()
        } else {
            self.theme.prompt_symbol.paint(&git_branch).to_string()
        };
        let git_prompt_s = if git_branch.is_empty() {
            String::new()
        } else {
            self.git_prompt_format
                .as_deref()
                .unwrap_or("{git_branch}")
                .replace("{git_branch}", &git_branch_s)
        };

        template
            .replace("{user}", &user_s)
            .replace("{host}", &host_s)
            .replace("{cwd}", &dir_s)
            .replace("{symbol}", &sym_s)
            .replace("{git_prompt}", &git_prompt_s)
            .replace("{git_branch}", &git_branch_s)
            .replace("%#", &sym_s)
            .replace("%n", &user)
            .replace("%m", &host)
            .replace("%~", &cwd)
    }
}

fn current_git_branch() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    git_branch_from_dir(&cwd)
}

fn git_branch_from_dir(start: &Path) -> Option<String> {
    for dir in start.ancestors() {
        let git_path = dir.join(".git");
        if git_path.is_dir() {
            return git_branch_from_git_dir(&git_path);
        }
        if git_path.is_file() {
            let git_dir = read_gitdir_file(&git_path)?;
            let git_dir = if git_dir.is_absolute() {
                git_dir
            } else {
                dir.join(git_dir)
            };
            return git_branch_from_git_dir(&git_dir);
        }
    }
    None
}

fn read_gitdir_file(path: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(path).ok()?;
    let gitdir = content.trim().strip_prefix("gitdir:")?.trim();
    if gitdir.is_empty() {
        None
    } else {
        Some(PathBuf::from(gitdir))
    }
}

fn git_branch_from_git_dir(git_dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        return (!branch.is_empty()).then(|| branch.to_string());
    }
    if head.len() >= 7 && head.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Some(head.chars().take(7).collect());
    }
    None
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
            None,
            "default",
        );

        assert_eq!(prompt.render_prompt_right(), "right");
    }

    #[test]
    fn omits_right_prompt_when_unset() {
        let prompt = WinuxshPrompt::new(Some("left> ".to_string()), None, None, "default");

        assert_eq!(prompt.render_prompt_right(), "");
    }

    #[test]
    fn reads_branch_from_git_head() {
        let dir = unique_temp_dir("winuxsh-prompt-git-head");
        let git_dir = dir.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        assert_eq!(git_branch_from_dir(&dir).as_deref(), Some("main"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn renders_git_prompt_only_inside_git_repo() {
        let dir = unique_temp_dir("winuxsh-prompt-git-render");
        let git_dir = dir.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feature/demo\n").unwrap();
        let _cwd = CwdGuard::enter(&dir);

        let prompt = WinuxshPrompt::new(
            Some("{git_prompt}".to_string()),
            None,
            Some("git:({git_branch})".to_string()),
            "default",
        );

        let rendered = prompt.render_prompt_left();
        assert!(rendered.contains("git:("));
        assert!(rendered.contains("feature/demo"));
        assert!(rendered.contains(")"));

        let _ = std::fs::remove_dir_all(dir);
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
    }

    struct CwdGuard {
        previous: PathBuf,
    }

    impl CwdGuard {
        fn enter(path: &Path) -> Self {
            let previous = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { previous }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.previous);
        }
    }
}
