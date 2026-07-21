//! Native zsh-syntax-highlighting-style line highlighter.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use nu_ansi_term::{Color, Style};
use reedline::{Highlighter, StyledText};

use crate::autosuggest::parse_zsh_style;
use crate::completion::command::CommandCompleter;
use crate::config::SyntaxHighlightConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SyntaxKind {
    Default,
    UnknownToken,
    ReservedWord,
    Builtin,
    Command,
    CommandSeparator,
    Path,
    PathPrefix,
    SingleQuotedArgument,
    DoubleQuotedArgument,
    Variable,
    CommandSubstitution,
    SingleHyphenOption,
    DoubleHyphenOption,
    Assign,
    Redirection,
    Comment,
}

impl SyntaxKind {
    fn zsh_key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::UnknownToken => "unknown-token",
            Self::ReservedWord => "reserved-word",
            Self::Builtin => "builtin",
            Self::Command => "command",
            Self::CommandSeparator => "commandseparator",
            Self::Path => "path",
            Self::PathPrefix => "path_prefix",
            Self::SingleQuotedArgument => "single-quoted-argument",
            Self::DoubleQuotedArgument => "double-quoted-argument",
            Self::Variable => "dollar-double-quoted-argument",
            Self::CommandSubstitution => "command-substitution",
            Self::SingleHyphenOption => "single-hyphen-option",
            Self::DoubleHyphenOption => "double-hyphen-option",
            Self::Assign => "assign",
            Self::Redirection => "redirection",
            Self::Comment => "comment",
        }
    }
}

/// Reedline highlighter that implements a conservative subset of zsh's `main`
/// highlighter.
pub struct WinuxshSyntaxHighlighter {
    styles: HashMap<SyntaxKind, Style>,
    commands: HashSet<String>,
    max_length: Option<usize>,
}

impl WinuxshSyntaxHighlighter {
    pub fn new(config: &SyntaxHighlightConfig) -> Self {
        let mut styles = default_styles();
        for (key, value) in &config.styles {
            if let Some(kind) = kind_from_zsh_key(key) {
                let default = styles.get(&kind).copied().unwrap_or_default();
                styles.insert(kind, parse_zsh_style(value, default));
            }
        }

        Self {
            styles,
            commands: CommandCompleter::get_all_commands()
                .into_iter()
                .map(|command| command.to_ascii_lowercase())
                .collect(),
            max_length: config.max_length,
        }
    }

    fn style(&self, kind: SyntaxKind) -> Style {
        self.styles.get(&kind).copied().unwrap_or_default()
    }
}

impl Highlighter for WinuxshSyntaxHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        if self
            .max_length
            .map(|max| line.chars().count() > max)
            .unwrap_or(false)
        {
            return plain_text(line);
        }

        let tokens = lex(line);
        if tokens.is_empty() {
            return plain_text(line);
        }

        let mut styled = StyledText::new();
        let mut pos = 0;
        let mut expect_command = true;
        let mut after_redirection = false;

        for token in tokens {
            if pos < token.start {
                styled.push((self.style(SyntaxKind::Default), line[pos..token.start].to_string()));
            }

            let kind = match token.kind {
                TokenKind::Comment => SyntaxKind::Comment,
                TokenKind::CommandSeparator => {
                    expect_command = true;
                    after_redirection = false;
                    SyntaxKind::CommandSeparator
                }
                TokenKind::Redirection => {
                    after_redirection = true;
                    SyntaxKind::Redirection
                }
                TokenKind::Word => {
                    let classified =
                        classify_word(&token.text, expect_command, after_redirection, self);
                    if expect_command && classified != SyntaxKind::Assign {
                        expect_command = false;
                    }
                    after_redirection = false;
                    classified
                }
            };

            styled.push((self.style(kind), token.text));
            pos = token.end;
        }

        if pos < line.len() {
            styled.push((self.style(SyntaxKind::Default), line[pos..].to_string()));
        }

        styled
    }
}

fn plain_text(line: &str) -> StyledText {
    let mut styled = StyledText::new();
    styled.push((Style::new(), line.to_string()));
    styled
}

fn classify_word(
    word: &str,
    expect_command: bool,
    after_redirection: bool,
    highlighter: &WinuxshSyntaxHighlighter,
) -> SyntaxKind {
    let unquoted = unquote_word(word);

    if expect_command && is_assignment(&unquoted) {
        return SyntaxKind::Assign;
    }

    if expect_command {
        if is_reserved_word(&unquoted) {
            return SyntaxKind::ReservedWord;
        }
        if is_shell_builtin(&unquoted) {
            return SyntaxKind::Builtin;
        }
        if highlighter.commands.contains(&unquoted.to_ascii_lowercase()) {
            return SyntaxKind::Command;
        }
        return SyntaxKind::UnknownToken;
    }

    if after_redirection {
        return path_kind(&unquoted).unwrap_or(SyntaxKind::Default);
    }
    if word.starts_with('\'') {
        return SyntaxKind::SingleQuotedArgument;
    }
    if word.starts_with('"') {
        return SyntaxKind::DoubleQuotedArgument;
    }
    if word.contains("$(") || word.contains('`') {
        return SyntaxKind::CommandSubstitution;
    }
    if contains_variable(word) {
        return SyntaxKind::Variable;
    }
    if word.starts_with("--") {
        return SyntaxKind::DoubleHyphenOption;
    }
    if word.starts_with('-') && word.len() > 1 {
        return SyntaxKind::SingleHyphenOption;
    }
    if is_assignment(&unquoted) {
        return SyntaxKind::Assign;
    }
    path_kind(&unquoted).unwrap_or(SyntaxKind::Default)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    start: usize,
    end: usize,
    text: String,
    kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    CommandSeparator,
    Redirection,
    Comment,
}

fn lex(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let ch = next_char(line, index);
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }

        if ch == '#' && comment_can_start(line, index) {
            tokens.push(Token {
                start: index,
                end: line.len(),
                text: line[index..].to_string(),
                kind: TokenKind::Comment,
            });
            break;
        }

        if let Some(end) = command_separator_end(line, index) {
            tokens.push(Token {
                start: index,
                end,
                text: line[index..end].to_string(),
                kind: TokenKind::CommandSeparator,
            });
            index = end;
            continue;
        }

        if let Some(end) = redirection_end(line, index) {
            tokens.push(Token {
                start: index,
                end,
                text: line[index..end].to_string(),
                kind: TokenKind::Redirection,
            });
            index = end;
            continue;
        }

        let end = word_end(line, index);
        tokens.push(Token {
            start: index,
            end,
            text: line[index..end].to_string(),
            kind: TokenKind::Word,
        });
        index = end;
    }

    tokens
}

fn next_char(line: &str, index: usize) -> char {
    line[index..].chars().next().unwrap()
}

fn comment_can_start(line: &str, index: usize) -> bool {
    index == 0 || line[..index].chars().last().is_some_and(char::is_whitespace)
}

fn command_separator_end(line: &str, index: usize) -> Option<usize> {
    for separator in ["&&", "||", "|", ";"] {
        if line[index..].starts_with(separator) {
            return Some(index + separator.len());
        }
    }
    None
}

fn redirection_end(line: &str, index: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut probe = index;

    while probe < line.len() && bytes[probe].is_ascii_digit() {
        probe += 1;
    }

    if probe >= line.len() {
        return None;
    }

    let ch = next_char(line, probe);
    if ch != '<' && ch != '>' {
        return None;
    }

    probe += ch.len_utf8();
    while probe < line.len() {
        let ch = next_char(line, probe);
        if ch.is_whitespace() || matches!(ch, ';' | '|') {
            break;
        }
        if ch == '<' || ch == '>' || ch == '&' || ch.is_ascii_digit() || ch == '-' {
            probe += ch.len_utf8();
        } else {
            break;
        }
    }

    Some(probe)
}

fn word_end(line: &str, start: usize) -> usize {
    let mut index = start;
    let mut quote = None;
    let mut escaped = false;

    while index < line.len() {
        let ch = next_char(line, index);
        if escaped {
            escaped = false;
            index += ch.len_utf8();
            continue;
        }

        if ch == '\\' {
            escaped = true;
            index += ch.len_utf8();
            continue;
        }

        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            index += ch.len_utf8();
            continue;
        }

        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            index += ch.len_utf8();
            continue;
        }

        if ch.is_whitespace()
            || ch == '#'
            || command_separator_end(line, index).is_some()
            || redirection_end(line, index).is_some()
        {
            break;
        }

        index += ch.len_utf8();
    }

    index
}

fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch == '_' || ch.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_reserved_word(word: &str) -> bool {
    matches!(
        word,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "for"
            | "in"
            | "do"
            | "done"
            | "while"
            | "until"
            | "case"
            | "esac"
            | "function"
            | "{"
            | "}"
    )
}

fn is_shell_builtin(word: &str) -> bool {
    matches!(
        word,
        "alias"
            | "bg"
            | "break"
            | "cd"
            | "continue"
            | "echo"
            | "eval"
            | "exec"
            | "exit"
            | "export"
            | "false"
            | "fg"
            | "help"
            | "history"
            | "jobs"
            | "pwd"
            | "read"
            | "return"
            | "set"
            | "shift"
            | "source"
            | "test"
            | "true"
            | "type"
            | "unalias"
            | "unset"
    )
}

fn contains_variable(word: &str) -> bool {
    let mut chars = word.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            match chars.peek() {
                Some('{') | Some('_') => return true,
                Some(next) if next.is_ascii_alphabetic() => return true,
                _ => {}
            }
        }
    }
    false
}

fn path_kind(word: &str) -> Option<SyntaxKind> {
    let path = resolve_path(word);
    if path.exists() {
        return Some(SyntaxKind::Path);
    }
    if path_prefix_exists(&path) {
        return Some(SyntaxKind::PathPrefix);
    }
    None
}

fn resolve_path(word: &str) -> PathBuf {
    let word = word.trim_matches(|ch| matches!(ch, '\'' | '"' | ')' | '('));
    let expanded = if word == "~" {
        dirs::home_dir()
    } else if let Some(rest) = word.strip_prefix("~/").or_else(|| word.strip_prefix("~\\")) {
        dirs::home_dir().map(|home| home.join(rest))
    } else {
        None
    };

    let path = expanded.unwrap_or_else(|| PathBuf::from(word));
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn path_prefix_exists(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if file_name.is_empty() {
        return false;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let Ok(entries) = std::fs::read_dir(parent) else {
        return false;
    };

    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .map(|candidate| candidate.starts_with(file_name))
            .unwrap_or(false)
    })
}

fn unquote_word(word: &str) -> String {
    if word.len() >= 2 {
        let bytes = word.as_bytes();
        if (bytes[0] == b'\'' && bytes[word.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[word.len() - 1] == b'"')
        {
            return word[1..word.len() - 1].to_string();
        }
    }
    word.to_string()
}

fn kind_from_zsh_key(key: &str) -> Option<SyntaxKind> {
    let key = key.to_ascii_lowercase();
    [
        SyntaxKind::Default,
        SyntaxKind::UnknownToken,
        SyntaxKind::ReservedWord,
        SyntaxKind::Builtin,
        SyntaxKind::Command,
        SyntaxKind::CommandSeparator,
        SyntaxKind::Path,
        SyntaxKind::PathPrefix,
        SyntaxKind::SingleQuotedArgument,
        SyntaxKind::DoubleQuotedArgument,
        SyntaxKind::Variable,
        SyntaxKind::CommandSubstitution,
        SyntaxKind::SingleHyphenOption,
        SyntaxKind::DoubleHyphenOption,
        SyntaxKind::Assign,
        SyntaxKind::Redirection,
        SyntaxKind::Comment,
    ]
    .into_iter()
    .find(|kind| kind.zsh_key() == key)
}

fn default_styles() -> HashMap<SyntaxKind, Style> {
    HashMap::from([
        (SyntaxKind::Default, Style::new()),
        (SyntaxKind::UnknownToken, Style::new().fg(Color::Red)),
        (
            SyntaxKind::ReservedWord,
            Style::new().bold().fg(Color::Purple),
        ),
        (SyntaxKind::Builtin, Style::new().fg(Color::Cyan)),
        (SyntaxKind::Command, Style::new().fg(Color::Green)),
        (SyntaxKind::CommandSeparator, Style::new().bold()),
        (SyntaxKind::Path, Style::new().underline().fg(Color::Cyan)),
        (SyntaxKind::PathPrefix, Style::new().fg(Color::Cyan)),
        (SyntaxKind::SingleQuotedArgument, Style::new().fg(Color::Yellow)),
        (SyntaxKind::DoubleQuotedArgument, Style::new().fg(Color::Yellow)),
        (SyntaxKind::Variable, Style::new().fg(Color::LightPurple)),
        (
            SyntaxKind::CommandSubstitution,
            Style::new().fg(Color::LightPurple),
        ),
        (SyntaxKind::SingleHyphenOption, Style::new().fg(Color::Blue)),
        (SyntaxKind::DoubleHyphenOption, Style::new().fg(Color::Blue)),
        (SyntaxKind::Assign, Style::new().fg(Color::LightCyan)),
        (SyntaxKind::Redirection, Style::new().fg(Color::Yellow)),
        (SyntaxKind::Comment, Style::new().fg(Color::DarkGray)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn highlights_known_and_unknown_commands() {
        let highlighter = WinuxshSyntaxHighlighter::new(&SyntaxHighlightConfig::default());
        let styled = highlighter.highlight("ls; definitely-not-a-winuxsh-command", 0);

        assert_eq!(style_for(&styled, "ls"), highlighter.style(SyntaxKind::Command));
        assert_eq!(
            style_for(&styled, "definitely-not-a-winuxsh-command"),
            highlighter.style(SyntaxKind::UnknownToken)
        );
    }

    #[test]
    fn highlights_main_syntax_tokens() {
        let highlighter = WinuxshSyntaxHighlighter::new(&SyntaxHighlightConfig::default());
        let styled = highlighter.highlight("echo \"$HOME\" | grep --ignore-case foo # note", 0);

        assert_eq!(style_for(&styled, "echo"), highlighter.style(SyntaxKind::Builtin));
        assert_eq!(
            style_for(&styled, "\"$HOME\""),
            highlighter.style(SyntaxKind::DoubleQuotedArgument)
        );
        assert_eq!(
            style_for(&styled, "|"),
            highlighter.style(SyntaxKind::CommandSeparator)
        );
        assert_eq!(
            style_for(&styled, "--ignore-case"),
            highlighter.style(SyntaxKind::DoubleHyphenOption)
        );
        assert_eq!(style_for(&styled, "# note"), highlighter.style(SyntaxKind::Comment));
    }

    #[test]
    fn highlights_existing_paths_and_prefixes() {
        let temp = unique_temp_dir("winuxsh-highlight-path");
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("sample.txt");
        std::fs::write(&file, "ok").unwrap();
        let prefix = temp.join("sam");
        let line = format!("cat {} {}", file.display(), prefix.display());

        let highlighter = WinuxshSyntaxHighlighter::new(&SyntaxHighlightConfig::default());
        let styled = highlighter.highlight(&line, 0);

        assert_eq!(
            style_for(&styled, &file.display().to_string()),
            highlighter.style(SyntaxKind::Path)
        );
        assert_eq!(
            style_for(&styled, &prefix.display().to_string()),
            highlighter.style(SyntaxKind::PathPrefix)
        );

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn honors_style_overrides_and_max_length() {
        let mut config = SyntaxHighlightConfig {
            max_length: Some(2),
            ..Default::default()
        };
        config
            .styles
            .insert("command".to_string(), "fg=magenta,bold".to_string());

        let highlighter = WinuxshSyntaxHighlighter::new(&config);
        assert_eq!(
            highlighter.style(SyntaxKind::Command).foreground,
            Some(Color::Magenta)
        );
        assert!(highlighter.style(SyntaxKind::Command).is_bold);
        assert_eq!(highlighter.highlight("echo", 0).buffer[0].0, Style::new());
    }

    fn style_for(styled: &StyledText, fragment: &str) -> Style {
        styled
            .buffer
            .iter()
            .find(|(_, text)| text == fragment)
            .or_else(|| styled.buffer.iter().find(|(_, text)| text.contains(fragment)))
            .map(|(style, _)| *style)
            .unwrap_or_else(|| panic!("missing fragment {fragment:?} in {:?}", raw_parts(styled)))
    }

    fn raw_parts(styled: &StyledText) -> Vec<String> {
        styled.buffer.iter().map(|(_, text)| text.clone()).collect()
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
    }
}
