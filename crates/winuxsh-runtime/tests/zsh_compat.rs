use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use winuxsh_runtime::config::ZshCompatLevel;
use winuxsh_runtime::zsh_compat::{
    apply_safe_aliases, apply_safe_env, completion_defs_from_report, safe_path_value,
    CompletionAsset, DiagnosticSeverity, ImportedAlias, ImportedEnv, ZshImportOptions,
    ZshImportReport, PluginImportKind, PluginImportTier, scan,
};

#[test]
fn scans_zshrc_and_oh_my_zsh_plugin_assets() {
    let temp = unique_temp_dir("winuxsh-zsh-compat");
    let omz = temp.join(".oh-my-zsh");
    let git_plugin_dir = omz.join("plugins").join("git");
    let alias_plugin_dir = omz.join("plugins").join("alias-only");
    let completion_plugin_dir = omz.join("plugins").join("completion-only");
    let native_plugin_dir = omz.join("plugins").join("zsh-autosuggestions");
    std::fs::create_dir_all(&git_plugin_dir).unwrap();
    std::fs::create_dir_all(&alias_plugin_dir).unwrap();
    std::fs::create_dir_all(&completion_plugin_dir).unwrap();
    std::fs::create_dir_all(&native_plugin_dir).unwrap();

    std::fs::write(
        git_plugin_dir.join("git.plugin.zsh"),
        r#"
alias gst='git status'
compdef _git gst=git-status
zle -N git_widget
"#,
    )
    .unwrap();
    std::fs::write(git_plugin_dir.join("_git"), "#compdef git gst\n").unwrap();
    std::fs::write(
        alias_plugin_dir.join("alias-only.plugin.zsh"),
        "alias gco='git checkout'\n",
    )
    .unwrap();
    std::fs::write(
        completion_plugin_dir.join("_ztarget"),
        "#compdef ztarget\n_arguments '--example[example flag]'\n",
    )
    .unwrap();
    std::fs::write(
        native_plugin_dir.join("zsh-autosuggestions.plugin.zsh"),
        "BUFFER=${BUFFER}\n",
    )
    .unwrap();

    let omz_text = omz.to_string_lossy().replace('\\', "/");
    std::fs::write(
        temp.join(".zshrc"),
        format!(
            r#"
export ZSH="{}"
ZSH_THEME="robbyrussell"
plugins=(git alias-only completion-only zsh-autosuggestions missing-plugin)
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
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "gco" && alias.value == "git checkout" && alias.origin == "plugin"
    }));

    let git = plugin(&report, "git");
    assert!(git.source_dir.is_some());
    assert_eq!(git.import_kind, PluginImportKind::Partial);
    assert_eq!(git.tier, PluginImportTier::Tier2Partial);
    assert!(git.capabilities.iter().any(|cap| cap == "aliases"));
    assert!(git.capabilities.iter().any(|cap| cap == "static_completions"));
    assert!(git
        .unsupported_features
        .iter()
        .any(|feature| feature == "zle"));

    let alias_only = plugin(&report, "alias-only");
    assert_eq!(alias_only.import_kind, PluginImportKind::AliasOnly);
    assert_eq!(alias_only.tier, PluginImportTier::Tier1Safe);

    let completion_only = plugin(&report, "completion-only");
    assert_eq!(
        completion_only.import_kind,
        PluginImportKind::CompletionOnly
    );
    assert_eq!(completion_only.tier, PluginImportTier::Tier1Safe);

    let native = plugin(&report, "zsh-autosuggestions");
    assert_eq!(native.import_kind, PluginImportKind::NativeUx);
    assert_eq!(native.tier, PluginImportTier::Tier3Native);
    assert!(native
        .capabilities
        .iter()
        .any(|capability| capability == "native_ux_required"));

    let missing = plugin(&report, "missing-plugin");
    assert_eq!(missing.import_kind, PluginImportKind::Missing);
    assert_eq!(missing.tier, PluginImportTier::Missing);

    assert!(report
        .completion_assets
        .iter()
        .any(|asset| asset.commands.iter().any(|cmd| cmd == "git")));
    assert!(report
        .completion_assets
        .iter()
        .any(|asset| asset.commands.iter().any(|cmd| cmd == "ztarget")));
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

#[test]
fn translates_static_zsh_arguments_completion_assets() {
    let temp = unique_temp_dir("winuxsh-zsh-completion-translate");
    std::fs::create_dir_all(&temp).unwrap();
    let completion = temp.join("_autopep8");
    std::fs::write(
        &completion,
        r#"
#compdef autopep8 ap8
_arguments -s -S \
  "--help[show this help message and exit]:" \
  "-h[show this help message and exit]:" \
  "{--diff,-d}[print the diff for the fixed source]" \
  "--jobs[number of parallel jobs]::jobs:_files" \
  "*::args:_files"
"#,
    )
    .unwrap();

    let report = ZshImportReport {
        completion_assets: vec![CompletionAsset {
            source_file: completion,
            commands: vec!["autopep8".to_string(), "ap8".to_string()],
            kind: "#compdef".to_string(),
        }],
        ..Default::default()
    };

    let defs = completion_defs_from_report(&report);
    assert_eq!(defs.len(), 2);

    let autopep8 = defs.iter().find(|def| def.command == "autopep8").unwrap();
    assert!(autopep8.flags.iter().any(|flag| flag.long.as_deref() == Some("--help")));
    assert!(autopep8.flags.iter().any(|flag| flag.short.as_deref() == Some("-h")));
    assert!(autopep8.flags.iter().any(|flag| {
        flag.long.as_deref() == Some("--diff") && flag.short.as_deref() == Some("-d")
    }));
    assert!(autopep8.flags.iter().any(|flag| {
        flag.long.as_deref() == Some("--jobs") && flag.takes_value
    }));
    assert!(!autopep8.flags.iter().any(|flag| flag.short.as_deref() == Some("-s")));
    assert!(defs.iter().any(|def| def.command == "ap8"));

    let _ = std::fs::remove_dir_all(temp);
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos))
}

fn plugin<'a>(
    report: &'a ZshImportReport,
    name: &str,
) -> &'a winuxsh_runtime::zsh_compat::ImportedPlugin {
    report
        .plugins
        .iter()
        .find(|plugin| plugin.name == name)
        .unwrap_or_else(|| panic!("expected plugin {name}, got {:?}", report.plugins))
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
