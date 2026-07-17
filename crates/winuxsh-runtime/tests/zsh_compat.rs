use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use winuxsh_runtime::config::ZshCompatLevel;
use winuxsh_runtime::zsh_compat::{scan, DiagnosticSeverity, ZshImportOptions};

#[test]
fn scans_zshrc_and_oh_my_zsh_plugin_assets() {
    let temp = unique_temp_dir("winuxsh-zsh-compat");
    let omz = temp.join(".oh-my-zsh");
    let plugin_dir = omz.join("plugins").join("git");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    std::fs::write(
        plugin_dir.join("git.plugin.zsh"),
        r#"
alias gst='git status'
compdef _git gst=git-status
zle -N git_widget
"#,
    )
    .unwrap();
    std::fs::write(plugin_dir.join("_git"), "#compdef git gst\n").unwrap();

    let omz_text = omz.to_string_lossy().replace('\\', "/");
    std::fs::write(
        temp.join(".zshrc"),
        format!(
            r#"
export ZSH="{}"
ZSH_THEME="robbyrussell"
plugins=(git zsh-autosuggestions)
alias ll='ls -l'
export PATH="$HOME/bin:$PATH"
fpath=("$ZSH/custom/completions" $fpath)
bindkey -v
zstyle ':completion:*' matcher-list 'm:{{a-z}}={{A-Z}}'
source $ZSH/oh-my-zsh.sh
"#,
            omz_text
        ),
    )
    .unwrap();

    let report = scan(&ZshImportOptions {
        enabled: true,
        zdotdir: temp.clone(),
        import_zshrc: true,
        import_oh_my_zsh: true,
        plugins: Vec::new(),
        compat_level: ZshCompatLevel::Safe,
    });

    assert_eq!(report.theme.as_deref(), Some("robbyrussell"));
    assert_eq!(report.edit_mode.as_deref(), Some("vi"));
    assert!(report.oh_my_zsh_detected);
    assert!(report.aliases.iter().any(|alias| alias.name == "ll" && alias.value == "ls -l"));
    assert!(report.aliases.iter().any(|alias| alias.name == "gst" && alias.value == "git status"));
    assert!(report.plugins.iter().any(|plugin| plugin.name == "git" && plugin.source_dir.is_some()));
    assert!(report.plugins.iter().any(|plugin| plugin.name == "zsh-autosuggestions" && plugin.source_dir.is_none()));
    assert!(report
        .completion_assets
        .iter()
        .any(|asset| asset.commands.iter().any(|cmd| cmd == "git")));
    assert!(report
        .zstyles
        .iter()
        .any(|style| style.context == ":completion:*" && style.key == "matcher-list"));
    assert!(report
        .diagnostics
        .iter()
        .any(|diag| diag.severity == DiagnosticSeverity::Unsupported && diag.feature == "zle"));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn reports_unsupported_global_aliases_and_zmodload() {
    let temp = unique_temp_dir("winuxsh-zsh-unsupported");
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(
        temp.join(".zshrc"),
        r#"
alias -g G='| grep'
zmodload zsh/zpty
"#,
    )
    .unwrap();

    let report = scan(&ZshImportOptions {
        enabled: true,
        zdotdir: temp.clone(),
        import_zshrc: true,
        import_oh_my_zsh: false,
        plugins: Vec::new(),
        compat_level: ZshCompatLevel::Safe,
    });

    assert!(report.aliases.is_empty());
    assert!(report
        .diagnostics
        .iter()
        .any(|diag| diag.feature == "alias" && diag.severity == DiagnosticSeverity::Unsupported));
    assert!(report
        .diagnostics
        .iter()
        .any(|diag| diag.feature == "zmodload" && diag.severity == DiagnosticSeverity::Unsupported));

    let _ = std::fs::remove_dir_all(temp);
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
}