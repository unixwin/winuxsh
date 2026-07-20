//! First-run setup wizard (Oh-My-Zsh style).
//!
//! Guides the user through initial configuration when `~/.winshrc.toml`
//! does not exist, then writes the generated file.

use std::path::PathBuf;
use std::io::{self, Write};

use crate::theme;

/// Returns `true` if the user has never run the setup wizard before
/// (i.e. `~/.winshrc.toml` does not exist and `~/.winuxsh/.setup-done`
/// does not exist).
pub fn is_first_run() -> bool {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let config = home.join(".winshrc.toml");
    if config.is_file() {
        return false;
    }
    let flag = home.join(".winuxsh").join(".setup-done");
    if flag.is_file() {
        return false;
    }
    true
}

/// Run the interactive setup wizard.
///
/// Prints a welcome banner, asks the user a few questions with defaults,
/// writes `~/.winshrc.toml`, and creates the `.setup-done` marker.
pub fn run_wizard() -> anyhow::Result<()> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    println!();
    println!(
        " {}  Welcome to Winuxsh {}!",
        "\u{1f389}",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        " {}  A bash-compatible shell for Windows \u{2014} no WSL, no MSYS2 required.",
        "\u{2728}"
    );
    println!();
    println!("  Let\u{2019}s get you set up.  (Press Enter to accept defaults.)");
    println!();

    // --- Editor mode ---
    let edit_mode = prompt_choice(
        "  \u{1f4dd}  Editing mode",
        "emacs",
        &["emacs", "vi"],
        "  \u{2502}  emacs = standard keybindings (Ctrl+A/E/K, Tab completion, Ctrl+R search)\n  \u{2502}  vi    = vim-style insert/normal modes",
    );

    // --- Theme ---
    let theme_list: Vec<&str> = theme::list_names().to_vec();
    let theme = prompt_choice("  \u{1f3a8}  Colour theme", "default", &theme_list, "  \u{2502}  Choose the prompt colour scheme.  Try \u{2018}colorful\u{2019} for a vibrant prompt.");

    // --- Prompt symbol ---
    let symbol = prompt_choice(
        "  \u{1f3b5}  Prompt symbol",
        "\u{276f}",
        &["\u{276f}", "\u{3bb}", "\u{25b6}", "\u{24}", "%"],
        "  \u{2502}  Pick the character that ends your prompt line.\n  \u{2502}  \u{276f} heavy right-pointing angle (powerlevel10k style)\n  \u{2502}  \u{3bb} lambda (functional/minimal)\n  \u{2502}  \u{25b6} black right-pointing triangle\n  \u{2502}  $ dollar sign (classic bash)\n  \u{2502}  % percent sign (classic fish)",
    );

    // --- Prompt style ---
    let prompt_style = prompt_choice(
        "  \u{1f3b5}  Prompt style",
        "minimal",
        &["minimal", "classic", "powerline", "multiline"],
        "  \u{2502}  minimal   = user@host cwd $\n  \u{2502}  classic   = user@host cwd git_branch git_status $\n  \u{2502}  powerline = unicode arrows with git status on the right\n  \u{2502}  multiline = first line: user@host, second line: cwd git_branch $",
    );

    // --- Right prompt ---
    let right_prompt = prompt_choice(
        "  \u{23f1}\u{fe0f}  Right-side info",
        "time",
        &["off", "time", "full"],
        "  \u{2502}  off  = no right prompt\n  \u{2502}  time = show current time (HH:MM)\n  \u{2502}  full = time + git branch \u{2014} useful with powerline or minimal prompt",
    );

    // --- Git prompt ---
    let git_enabled = prompt_yn(
        "  \u{1f500}  Show git branch/status in the prompt",
        true,
    );

    // --- Generate config ---
    let config_content = generate_config(
        &edit_mode,
        &theme,
        &prompt_style,
        &right_prompt,
        &symbol,
        git_enabled,
    );

    // Write ~/.winshrc.toml
    let config_path = home.join(".winshrc.toml");
    {
        let mut file = std::fs::File::create(&config_path)?;
        file.write_all(config_content.as_bytes())?;
    }

    // Write .setup-done flag so subsequent starts skip the wizard
    let winuxsh_dir = home.join(".winuxsh");
    let _ = std::fs::create_dir_all(&winuxsh_dir);
    let _ = std::fs::write(winuxsh_dir.join(".setup-done"), b"");

    println!();
    println!("  \u{2705}  Configuration written to {}", config_path.display());
    println!();
    println!("  \u{1f680}  You can tweak these settings any time by editing that file.");
    println!("  \u{1f4a1}  See DOCS/getting-started.md for the full configuration reference.");
    println!();

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn generate_config(
    edit_mode: &str,
    theme: &str,
    prompt_style: &str,
    right_prompt: &str,
    symbol: &str,
    git_enabled: bool,
) -> String {
    let (prompt_template, right_template) = match (prompt_style, right_prompt) {
        ("powerline", "time") => (
            "{cwd} {git_prompt} ".to_string(),
            "{time} ".to_string(),
        ),
        ("powerline", "full") => (
            "{cwd} {git_prompt} ".to_string(),
            "{time} {git_branch} ".to_string(),
        ),
        ("powerline", _) => (
            "{cwd} {git_prompt} ".to_string(),
            String::new(),
        ),
        ("multiline", "time") => (
            "{user}@{host} {time}\n{cwd} {git_prompt} ".to_string(),
            String::new(),
        ),
        ("multiline", "full") => (
            "{user}@{host} {time}\n{cwd} {git_prompt} ".to_string(),
            "{git_branch} ".to_string(),
        ),
        ("multiline", _) => (
            "{user}@{host}\n{cwd} {git_prompt} ".to_string(),
            String::new(),
        ),
        ("classic", "time") => (
            "{user}@{host} {cwd} {git_prompt} ".to_string(),
            "{time} ".to_string(),
        ),
        ("classic", "full") => (
            "{user}@{host} {cwd} {git_prompt} ".to_string(),
            "{time} {git_branch} ".to_string(),
        ),
        ("classic", _) => (
            "{user}@{host} {cwd} {git_prompt} ".to_string(),
            String::new(),
        ),
        // minimal
        ("minimal", "time") => (
            "{cwd} ".to_string(),
            "{time} ".to_string(),
        ),
        ("minimal", "full") => (
            "{cwd} ".to_string(),
            "{time} {git_branch} ".to_string(),
        ),
        _ => (
            "{cwd} ".to_string(),
            String::new(),
        ),
    };

    let git_section = if git_enabled {
        // oh-my-zsh style: `git:(branch) status`. The classic / multiline /
        // powerline templates all print `{git_prompt}` and rely on this
        // wrapper to look like the reference screenshot. Users who prefer a
        // bare branch name can delete this line.
        let git_format = match prompt_style {
            "minimal" => String::new(),
            _ => "git:({git_branch})".to_string(),
        };
        let git_format_line = if git_format.is_empty() {
            String::new()
        } else {
            format!("git_prompt_format = {:?}\n", git_format)
        };
        format!(
            "\n[git_prompt]\n{}# staged = \"\u{25cf}{{n}}\"   # uncomment to show counts\n# unstaged = \"\u{271a}{{n}}\"\n# separator = \" \"\n",
            git_format_line
        )
    } else {
        // Git explicitly disabled: blank all symbol formats so the segment
        // stays silent even if the renderer tries to render it.
        r#"
[git_prompt]
staged = ""
unstaged = ""
untracked = ""
deleted = ""
ahead = ""
behind = ""
stashes = ""
conflicts = ""
separator = " "
"#.to_string()
    };

    let indicator = format!("{} ", symbol);
    format!(
        r#"# Winuxsh configuration — generated by the setup wizard.
# Edit this file any time to customise your shell.

[shell]
prompt_format = {:?}
right_prompt_format = {:?}
prompt_symbol = {:?}
prompt_indicator = {:?}
emacs_indicator = {:?}
vi_insert_indicator = {:?}
vi_normal_indicator = {:?}
multiline_indicator = "> "

[editor]
edit_mode = {:?}

[theme]
current_theme = {:?}

[history]
max_size = 10000

[menus]
completion_page_size = 10
history_page_size = 10
max_entry_lines = 5
{}
"#,
            prompt_template,
            right_template,
            symbol,
            indicator,
            indicator,
            indicator,
            indicator,
            edit_mode,
            theme,
            git_section,
        ).replace("{sym}", symbol)
}

fn prompt_choice(label: &str, default: &str, options: &[&str], help: &str) -> String {
    println!("{}", label);
    for line in help.lines() {
        println!("{}", line);
    }

    let default_idx = options.iter().position(|o| *o == default).unwrap_or(0);
    let default_display = default_idx + 1;

    loop {
        print!("  |  Enter choice [1-{} / Enter for {}]: ", options.len(), default_display);
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let input = input.trim().to_lowercase();

        if input.is_empty() {
            return default.to_string();
        }

        if let Ok(idx) = input.parse::<usize>() {
            if idx >= 1 && idx <= options.len() {
                return options[idx - 1].to_string();
            }
        }

        if options.contains(&input.as_str()) {
            return input;
        }

        println!("  |  Enter a number 1-{} (or Enter for {}).", options.len(), default_display);
    }
}

fn prompt_yn(label: &str, default: bool) -> bool {
    let default_str = if default { "Y/n" } else { "y/N" };
    print!("  {} [{}]: ", label, default_str);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let input = input.trim().to_lowercase();

    match input.as_str() {
        "y" | "yes" | "true" => true,
        "n" | "no" | "false" => false,
        _ => default,
    }
}
