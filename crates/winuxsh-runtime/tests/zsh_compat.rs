use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use winuxsh_runtime::config::ZshCompatLevel;
use winuxsh_runtime::zsh_compat::{
    apply_safe_aliases, apply_safe_env, safe_path_value, DiagnosticSeverity, ImportedAlias,
    ImportedEnv, ZshImportOptions, ZshImportReport, scan,
};

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

#[test]
fn safe_path_value_prepends_imported_entries_and_dedupes() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvGuard::capture(&["PATH"]);

    let existing = test_path("existing-bin");
    let imported = test_path("zsh-bin");
    let imported_duplicate = duplicate_case_path(&imported);

    std::env::set_var(
        "PATH",
        std::env::join_paths([existing.clone(), imported.clone()]).unwrap(),
    );

    let report = ZshImportReport {
        path_entries: vec![imported.clone(), imported_duplicate],
        ..Default::default()
    };

    let path = safe_path_value(&report).unwrap();
    let entries: Vec<PathBuf> = std::env::split_paths(&path).collect();

    assert_eq!(entries.first(), Some(&imported));
    assert!(entries.iter().any(|entry| same_path_key(entry, &existing)));
    assert_eq!(
        entries
            .iter()
            .filter(|entry| same_path_key(entry, &imported))
            .count(),
        1
    );
}

#[test]
fn apply_safe_env_sets_safe_keys_and_skips_shell_internals() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvGuard::capture(&["WINUXSH_ZSH_SAFE_ENV", "ZSH_THEME", "BASHOPTS", "PATH"]);

    std::env::set_var("PATH", "baseline-path");
    std::env::remove_var("WINUXSH_ZSH_SAFE_ENV");
    std::env::remove_var("ZSH_THEME");
    std::env::remove_var("BASHOPTS");

    let report = ZshImportReport {
        env: vec![
            imported_env("WINUXSH_ZSH_SAFE_ENV", "ok"),
            imported_env("ZSH_THEME", "robbyrussell"),
            imported_env("BASHOPTS", "unsafe"),
            imported_env("PATH", "raw-path-should-not-win"),
            imported_env("__RUBASH_PRIVATE", "unsafe"),
        ],
        ..Default::default()
    };

    let summary = apply_safe_env(&report);

    assert_eq!(summary.env_applied, 2);
    assert_eq!(std::env::var("WINUXSH_ZSH_SAFE_ENV").as_deref(), Ok("ok"));
    assert_eq!(std::env::var("ZSH_THEME").as_deref(), Ok("robbyrussell"));
    assert!(std::env::var("BASHOPTS").is_err());
    assert_eq!(std::env::var("PATH").as_deref(), Ok("baseline-path"));
    assert!(std::env::var("__RUBASH_PRIVATE").is_err());
}

#[test]
fn apply_safe_aliases_installs_rubash_aliases() {
    let temp = unique_temp_dir("winuxsh-zsh-alias-apply");
    std::fs::create_dir_all(&temp).unwrap();
    let output = temp.join("alias-output.txt");
    let output_shell_path = output.to_string_lossy().replace('\\', "/");

    let report = ZshImportReport {
        aliases: vec![
            imported_alias("zll", "echo zsh-alias-ok"),
            imported_alias("bad/name", "echo should-not-apply"),
        ],
        ..Default::default()
    };

    let mut executor = rubash::executor::Executor::new();
    let summary = apply_safe_aliases(&report, &mut executor);
    assert_eq!(summary.aliases_applied, 1);

    run_rubash_script(
        &mut executor,
        &format!("shopt -s expand_aliases\nzll > {}", shell_quote(&output_shell_path)),
    );

    assert_eq!(
        std::fs::read_to_string(&output).unwrap().trim(),
        "zsh-alias-ok"
    );

    let _ = std::fs::remove_dir_all(temp);
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvGuard {
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    fn capture(names: &[&str]) -> Self {
        Self {
            saved: names
                .iter()
                .map(|name| ((*name).to_string(), std::env::var_os(name)))
                .collect(),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn imported_alias(name: &str, value: &str) -> ImportedAlias {
    ImportedAlias {
        name: name.to_string(),
        value: value.to_string(),
        source_file: None,
        line: None,
        origin: "test".to_string(),
    }
}

fn imported_env(key: &str, value: &str) -> ImportedEnv {
    ImportedEnv {
        key: key.to_string(),
        value: value.to_string(),
        source_file: None,
        line: None,
    }
}

fn run_rubash_script(executor: &mut rubash::executor::Executor, script: &str) {
    let tokens = rubash::lexer::tokenize(script);
    let ast = rubash::parser::parse(&tokens);
    executor.execute_ast(&ast).unwrap();
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("winuxsh-zsh-compat-{}", name))
}

fn duplicate_case_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_ascii_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn same_path_key(left: &Path, right: &Path) -> bool {
    let left = left.to_string_lossy().replace('/', "\\");
    let right = right.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}
