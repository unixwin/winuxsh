use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use winuxsh_runtime::config::ZshCompatLevel;
use winuxsh_runtime::zsh_compat::{
    apply_import_plan_to_config_with_backup_suffix, apply_safe_aliases, apply_safe_env,
    completion_defs_from_report, dynamic_completion_defs_from_report_with_runner,
    dynamic_completion_defs_from_report_with_options, git_prompt_format_from_report,
    import_plan_toml, inspect_import_config_status, inspect_import_rollback_plan, safe_path_value,
    scan, translate_zsh_prompt, CompletionAsset, DiagnosticSeverity, DynamicCompletionRunOptions,
    DynamicCompletionSource, ImportedAlias, ImportedEnv, ImportedPlugin, PluginImportKind,
    PluginImportTier, ZshCompatDiagnostic,
    ZshImportApplyReadiness, ZshImportBlockState, ZshImportConfigStatus, ZshImportOptions,
    ZshImportReport, ZshImportRollbackPlan, ZSH_IMPORT_BLOCK_END, ZSH_IMPORT_BLOCK_START,
    zsh_compat_doctor_text,
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
ZSH_HIGHLIGHT_STYLES[path]='fg=cyan,underline'
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
        .highlight_styles
        .iter()
        .any(|style| style.key == "path" && style.value == "fg=cyan,underline"));
    assert!(report
        .diagnostics
        .iter()
        .any(|diag| diag.severity == DiagnosticSeverity::Unsupported && diag.feature == "zle"));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn imports_native_git_alias_pack_when_omz_plugin_dir_is_missing() {
    let temp = unique_temp_dir("winuxsh-zsh-native-git-pack");
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(
        temp.join(".zshrc"),
        r#"
plugins=(git)
"#,
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

    let git = plugin(&report, "git");
    assert!(git.source_dir.is_none());
    assert_eq!(git.import_kind, PluginImportKind::AliasOnly);
    assert_eq!(git.tier, PluginImportTier::Tier1Safe);
    assert!(git.alias_count >= 20);
    assert!(git.capabilities.iter().any(|cap| cap == "native_aliases"));
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "gst"
            && alias.value == "git status"
            && alias.origin == "native-plugin:git"
    }));
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "gco"
            && alias.value == "git checkout"
            && alias.origin == "native-plugin:git"
    }));

    let plan = import_plan_toml(
        &ZshImportOptions {
            enabled: true,
            zdotdir: temp.clone(),
            import_zshrc: true,
            import_oh_my_zsh: true,
            plugins: Vec::new(),
            compat_level: ZshCompatLevel::Safe,
        },
        &report,
    );
    assert!(plan.contains("\"gst\" = \"git status\""));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn native_git_alias_pack_does_not_override_user_aliases() {
    let temp = unique_temp_dir("winuxsh-zsh-native-git-pack-no-override");
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(
        temp.join(".zshrc"),
        r#"
plugins=(git)
alias gst='git status --short'
"#,
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

    let gst_aliases: Vec<_> = report
        .aliases
        .iter()
        .filter(|alias| alias.name == "gst")
        .collect();
    assert_eq!(gst_aliases.len(), 1);
    assert_eq!(gst_aliases[0].value, "git status --short");
    assert_eq!(gst_aliases[0].origin, "profile");
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "gco"
            && alias.value == "git checkout"
            && alias.origin == "native-plugin:git"
    }));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn imports_native_docker_alias_pack_when_omz_plugin_dir_is_missing() {
    let temp = unique_temp_dir("winuxsh-zsh-native-docker-pack");
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(
        temp.join(".zshrc"),
        r#"
plugins=(docker)
"#,
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

    let docker = plugin(&report, "docker");
    assert!(docker.source_dir.is_none());
    assert_eq!(docker.import_kind, PluginImportKind::AliasOnly);
    assert_eq!(docker.tier, PluginImportTier::Tier1Safe);
    assert!(docker.alias_count >= 35);
    assert!(docker
        .capabilities
        .iter()
        .any(|cap| cap == "native_aliases"));
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "dps" && alias.value == "docker ps" && alias.origin == "native-plugin:docker"
    }));
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "drit"
            && alias.value == "docker container run -it"
            && alias.origin == "native-plugin:docker"
    }));
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "drm!"
            && alias.value == "docker container rm -f"
            && alias.origin == "native-plugin:docker"
    }));

    let plan = import_plan_toml(
        &ZshImportOptions {
            enabled: true,
            zdotdir: temp.clone(),
            import_zshrc: true,
            import_oh_my_zsh: true,
            plugins: Vec::new(),
            compat_level: ZshCompatLevel::Safe,
        },
        &report,
    );
    assert!(plan.contains("\"dps\" = \"docker ps\""));
    assert!(plan.contains("\"drm!\" = \"docker container rm -f\""));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn native_docker_alias_pack_does_not_override_user_aliases() {
    let temp = unique_temp_dir("winuxsh-zsh-native-docker-pack-no-override");
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(
        temp.join(".zshrc"),
        r#"
plugins=(docker)
alias dps='docker ps --format table'
"#,
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

    let dps_aliases: Vec<_> = report
        .aliases
        .iter()
        .filter(|alias| alias.name == "dps")
        .collect();
    assert_eq!(dps_aliases.len(), 1);
    assert_eq!(dps_aliases[0].value, "docker ps --format table");
    assert_eq!(dps_aliases[0].origin, "profile");
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "drit"
            && alias.value == "docker container run -it"
            && alias.origin == "native-plugin:docker"
    }));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn reports_dynamic_completion_generators_in_plugin_scripts() {
    let temp = unique_temp_dir("winuxsh-zsh-dynamic-completion");
    let kubectl_plugin_dir = temp.join(".oh-my-zsh").join("plugins").join("kubectl");
    std::fs::create_dir_all(&kubectl_plugin_dir).unwrap();
    std::fs::write(
        temp.join(".zshrc"),
        r#"
plugins=(kubectl)
"#,
    )
    .unwrap();
    std::fs::write(
        kubectl_plugin_dir.join("kubectl.plugin.zsh"),
        r#"
if [[ ! -f "$ZSH_CACHE_DIR/completions/_kubectl" ]]; then
  typeset -g -A _comps
  autoload -Uz _kubectl
  _comps[kubectl]=_kubectl
fi

kubectl completion zsh 2> /dev/null >| "$ZSH_CACHE_DIR/completions/_kubectl" &|
alias k=kubectl
"#,
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

    let kubectl = plugin(&report, "kubectl");
    assert_eq!(kubectl.import_kind, PluginImportKind::Partial);
    assert_eq!(kubectl.tier, PluginImportTier::Tier2Partial);
    assert!(kubectl
        .capabilities
        .iter()
        .any(|cap| cap == "dynamic_completions_required"));
    assert!(kubectl
        .unsupported_features
        .iter()
        .any(|feature| feature == "dynamic-completion"));
    assert!(report.aliases.iter().any(|alias| {
        alias.name == "k" && alias.value == "kubectl" && alias.origin == "plugin"
    }));

    let source = report
        .dynamic_completion_sources
        .iter()
        .find(|source| source.command == "kubectl")
        .expect("expected kubectl dynamic completion source");
    assert_eq!(source.args, vec!["completion", "zsh"]);
    assert_eq!(source.target_shell, "zsh");
    assert_eq!(source.origin, "plugin");
    assert!(report.diagnostics.iter().any(|diag| {
        diag.severity == DiagnosticSeverity::Unsupported && diag.feature == "dynamic-completion"
    }));

    let plan = import_plan_toml(
        &ZshImportOptions {
            enabled: true,
            zdotdir: temp.clone(),
            import_zshrc: true,
            import_oh_my_zsh: true,
            plugins: Vec::new(),
            compat_level: ZshCompatLevel::Safe,
        },
        &report,
    );
    assert!(plan.contains("dynamic zsh completion generators detected: 1"));

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
fn scans_profile_prompts_and_static_oh_my_zsh_theme() {
    let temp = unique_temp_dir("winuxsh-zsh-prompt");
    let omz = temp.join(".oh-my-zsh");
    let theme_dir = omz.join("themes");
    std::fs::create_dir_all(&theme_dir).unwrap();
    std::fs::write(
        theme_dir.join("simple.zsh-theme"),
        "PROMPT='%F{blue}%n@%m:%2~ $(git_prompt_info)%f %# '\nRPROMPT='%D{%H:%M}'\nZSH_THEME_GIT_PROMPT_PREFIX='git:('\nZSH_THEME_GIT_PROMPT_SUFFIX=')'\n",
    )
    .unwrap();

    let omz_text = omz.to_string_lossy().replace('\\', "/");
    std::fs::write(
        temp.join(".zshrc"),
        format!(
            r#"
export ZSH="{}"
ZSH_THEME="simple"
PROMPT='%n@%m:%~ %# '
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

    let prompt = report.prompt.as_ref().unwrap();
    assert_eq!(prompt.origin, "profile");
    assert_eq!(
        prompt.translated_format.as_deref(),
        Some("{user}@{host}:{cwd} {symbol} ")
    );

    let right_prompt = report.right_prompt.as_ref().unwrap();
    assert_eq!(right_prompt.origin, "theme");
    assert_eq!(right_prompt.translated_format, None);
    assert!(right_prompt
        .unsupported_segments
        .iter()
        .any(|segment| segment == "%D{%H:%M}"));
    assert_eq!(
        git_prompt_format_from_report(&report).as_deref(),
        Some("git:({git_branch})")
    );

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn imports_theme_prompt_when_profile_prompt_is_absent() {
    let temp = unique_temp_dir("winuxsh-zsh-theme-prompt");
    let omz = temp.join(".oh-my-zsh");
    let theme_dir = omz.join("themes");
    std::fs::create_dir_all(&theme_dir).unwrap();
    std::fs::write(
        theme_dir.join("minimal.zsh-theme"),
        "PROMPT='%B%F{green}%3~%f%b \\$(git_prompt_info) %# '\nZSH_THEME_GIT_PROMPT_PREFIX='['\nZSH_THEME_GIT_PROMPT_SUFFIX=']'\n",
    )
    .unwrap();

    let omz_text = omz.to_string_lossy().replace('\\', "/");
    std::fs::write(
        temp.join(".zshrc"),
        format!(
            r#"
export ZSH="{}"
ZSH_THEME="minimal"
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

    let prompt = report.prompt.as_ref().unwrap();
    assert_eq!(prompt.origin, "theme");
    assert_eq!(
        prompt.translated_format.as_deref(),
        Some("{cwd} {git_prompt} {symbol} ")
    );
    assert_eq!(
        git_prompt_format_from_report(&report).as_deref(),
        Some("[{git_branch}]")
    );

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn translates_zsh_prompt_common_subset_and_reports_dynamic_segments() {
    let translation =
        translate_zsh_prompt("%B%F{blue}%n@%m:%3~ $(git_prompt_info)%f%b %# ");

    assert_eq!(
        translation.format.as_deref(),
        Some("{user}@{host}:{cwd} {git_prompt} {symbol} ")
    );
    assert!(!translation
        .unsupported_segments
        .iter()
        .any(|segment| segment == "$(git_prompt_info)"));

    let escaped = translate_zsh_prompt(r"%~ \$(git_prompt_info) %# ");
    assert_eq!(
        escaped.format.as_deref(),
        Some("{cwd} {git_prompt} {symbol} ")
    );
}

#[test]
fn emits_reviewable_import_plan_toml() {
    let temp = unique_temp_dir("winuxsh-zsh-import-plan");
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(
        temp.join(".zshrc"),
        r#"
plugins=(git zsh-autosuggestions)
alias ll='ls -l'
bindkey -v
PROMPT='%n:%~ %# '
RPROMPT='%m'
"#,
    )
    .unwrap();

    let options = ZshImportOptions {
        enabled: true,
        zdotdir: temp.clone(),
        import_zshrc: true,
        import_oh_my_zsh: false,
        plugins: Vec::new(),
        compat_level: ZshCompatLevel::Safe,
    };
    let report = scan(&options);
    let plan = import_plan_toml(&options, &report);

    toml::from_str::<toml::Value>(&plan).unwrap();
    assert!(plan.contains("[zsh]"));
    assert!(plan.contains("auto_apply = true"));
    assert!(plan.contains("plugins = [\"git\", \"zsh-autosuggestions\"]"));
    assert!(plan.contains("[editor]"));
    assert!(plan.contains("edit_mode = \"vi\""));
    assert!(plan.contains("prompt_format = \"{user}:{cwd} {symbol} \""));
    assert!(plan.contains("right_prompt_format = \"{host}\""));
    assert!(plan.contains("\"ll\" = \"ls -l\""));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn applies_import_plan_to_new_config_file() {
    let temp = unique_temp_dir("winuxsh-zsh-import-apply-new");
    std::fs::create_dir_all(&temp).unwrap();
    let config_path = temp.join(".winshrc.toml");
    let plan = r#"
[zsh]
enabled = true
auto_apply = true
"#;

    let summary =
        apply_import_plan_to_config_with_backup_suffix(&config_path, plan, "test-new").unwrap();
    let written = std::fs::read_to_string(&config_path).unwrap();

    assert!(!summary.replaced_existing_block);
    assert!(summary.backup_path.is_none());
    assert!(written.contains(ZSH_IMPORT_BLOCK_START));
    assert!(written.contains(ZSH_IMPORT_BLOCK_END));
    assert!(written.contains("auto_apply = true"));
    toml::from_str::<toml::Value>(&written).unwrap();

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn replaces_existing_import_plan_block_and_creates_backup() {
    let temp = unique_temp_dir("winuxsh-zsh-import-apply-replace");
    std::fs::create_dir_all(&temp).unwrap();
    let config_path = temp.join(".winshrc.toml");
    let original = format!(
        "theme = \"default\"\n\n{}\n[zsh]\nenabled = false\n{}\n",
        ZSH_IMPORT_BLOCK_START, ZSH_IMPORT_BLOCK_END
    );
    std::fs::write(&config_path, &original).unwrap();

    let plan = r#"
[zsh]
enabled = true
auto_apply = true
"#;
    let summary =
        apply_import_plan_to_config_with_backup_suffix(&config_path, plan, "test-replace")
            .unwrap();
    let written = std::fs::read_to_string(&config_path).unwrap();
    let backup_path = summary.backup_path.unwrap();

    assert!(summary.replaced_existing_block);
    assert_eq!(std::fs::read_to_string(&backup_path).unwrap(), original);
    assert!(written.contains("theme = \"default\""));
    assert!(written.contains("enabled = true"));
    assert!(!written.contains("enabled = false"));
    toml::from_str::<toml::Value>(&written).unwrap();

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn refuses_import_apply_when_generated_block_would_duplicate_tables() {
    let temp = unique_temp_dir("winuxsh-zsh-import-apply-duplicate");
    std::fs::create_dir_all(&temp).unwrap();
    let config_path = temp.join(".winshrc.toml");
    let original = "[zsh]\nenabled = false\n";
    std::fs::write(&config_path, original).unwrap();

    let err = apply_import_plan_to_config_with_backup_suffix(
        &config_path,
        "[zsh]\nenabled = true\n",
        "test-duplicate",
    )
    .unwrap_err();

    assert!(err.to_string().contains("generated zsh import block"));
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), original);
    assert!(!config_path
        .with_file_name(".winshrc.toml.test-duplicate.bak")
        .exists());

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn reports_import_status_for_missing_config() {
    let temp = unique_temp_dir("winuxsh-zsh-import-status-missing");
    std::fs::create_dir_all(&temp).unwrap();
    let config_path = temp.join(".winshrc.toml");

    let status =
        inspect_import_config_status(&config_path, "[zsh]\nenabled = true\n").unwrap();

    assert!(!status.config_exists);
    assert_eq!(status.block_state, ZshImportBlockState::Missing);
    assert!(status.toml_valid);
    assert_eq!(status.apply_readiness, ZshImportApplyReadiness::AddNewBlock);
    assert!(status.apply_error.is_none());
    assert!(status.backup_paths.is_empty());

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn reports_import_status_for_existing_block_and_backups() {
    let temp = unique_temp_dir("winuxsh-zsh-import-status-existing");
    std::fs::create_dir_all(&temp).unwrap();
    let config_path = temp.join(".winshrc.toml");
    std::fs::write(
        &config_path,
        format!(
            "{}\n[zsh]\nenabled = false\n{}\n",
            ZSH_IMPORT_BLOCK_START, ZSH_IMPORT_BLOCK_END
        ),
    )
    .unwrap();
    std::fs::write(temp.join(".winshrc.toml.100.bak"), "old").unwrap();

    let status =
        inspect_import_config_status(&config_path, "[zsh]\nenabled = true\n").unwrap();

    assert!(status.config_exists);
    assert_eq!(status.block_state, ZshImportBlockState::Present);
    assert!(status.toml_valid);
    assert_eq!(
        status.apply_readiness,
        ZshImportApplyReadiness::ReplaceExistingBlock
    );
    assert!(status.apply_error.is_none());
    assert_eq!(status.backup_paths.len(), 1);
    assert_eq!(status.backup_paths[0], temp.join(".winshrc.toml.100.bak"));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn reports_import_status_for_malformed_block() {
    let temp = unique_temp_dir("winuxsh-zsh-import-status-malformed");
    std::fs::create_dir_all(&temp).unwrap();
    let config_path = temp.join(".winshrc.toml");
    std::fs::write(
        &config_path,
        format!("{}\n[zsh]\nenabled = false\n", ZSH_IMPORT_BLOCK_START),
    )
    .unwrap();

    let status =
        inspect_import_config_status(&config_path, "[zsh]\nenabled = true\n").unwrap();

    assert_eq!(status.block_state, ZshImportBlockState::Malformed);
    assert!(status.toml_valid);
    assert_eq!(status.apply_readiness, ZshImportApplyReadiness::Blocked);
    assert!(status
        .apply_error
        .as_deref()
        .unwrap_or("")
        .contains("malformed"));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn reports_import_status_when_apply_would_duplicate_tables() {
    let temp = unique_temp_dir("winuxsh-zsh-import-status-duplicate");
    std::fs::create_dir_all(&temp).unwrap();
    let config_path = temp.join(".winshrc.toml");
    std::fs::write(&config_path, "[zsh]\nenabled = false\n").unwrap();

    let status =
        inspect_import_config_status(&config_path, "[zsh]\nenabled = true\n").unwrap();

    assert_eq!(status.block_state, ZshImportBlockState::Missing);
    assert!(status.toml_valid);
    assert_eq!(status.apply_readiness, ZshImportApplyReadiness::Blocked);
    assert!(status
        .apply_error
        .as_deref()
        .unwrap_or("")
        .contains("generated zsh import block"));
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        "[zsh]\nenabled = false\n"
    );

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn reports_empty_rollback_plan_without_backups() {
    let temp = unique_temp_dir("winuxsh-zsh-import-rollback-empty");
    std::fs::create_dir_all(&temp).unwrap();
    let config_path = temp.join(".winshrc.toml");

    let plan = inspect_import_rollback_plan(&config_path).unwrap();

    assert_eq!(plan.config_path, config_path);
    assert!(plan.backup_paths.is_empty());
    assert!(plan.latest_backup_path.is_none());
    assert!(plan.restore_command.is_none());

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn reports_latest_rollback_backup_and_restore_command() {
    let temp = unique_temp_dir("winuxsh-zsh-import-rollback-latest");
    let quoted_dir = temp.join("quote's home");
    std::fs::create_dir_all(&quoted_dir).unwrap();
    let config_path = quoted_dir.join(".winshrc.toml");
    let older_backup = quoted_dir.join(".winshrc.toml.100.bak");
    let latest_backup = quoted_dir.join(".winshrc.toml.200.bak");
    std::fs::write(&older_backup, "older").unwrap();
    std::fs::write(&latest_backup, "latest").unwrap();

    let plan = inspect_import_rollback_plan(&config_path).unwrap();

    assert_eq!(plan.backup_paths, vec![older_backup, latest_backup.clone()]);
    assert_eq!(plan.latest_backup_path, Some(latest_backup.clone()));
    let command = plan.restore_command.unwrap();
    assert!(command.starts_with("Copy-Item -LiteralPath "));
    assert!(command.contains("quote''s home"));
    assert!(command.contains(".winshrc.toml.200.bak"));
    assert!(command.ends_with(" -Force"));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn formats_doctor_summary_for_ready_import() {
    let config_path = PathBuf::from("C:/Users/me/.winshrc.toml");
    let backup_path = PathBuf::from("C:/Users/me/.winshrc.toml.100.bak");
    let report = ZshImportReport {
        source_files: vec![PathBuf::from("C:/Users/me/.zshrc")],
        aliases: vec![imported_alias("ll", "ls -l")],
        env: vec![imported_env("ZSH_THEME", "robbyrussell")],
        path_entries: vec![PathBuf::from("C:/Users/me/bin")],
        completion_assets: vec![CompletionAsset {
            source_file: PathBuf::from("C:/Users/me/.oh-my-zsh/plugins/git/_git"),
            commands: vec!["git".to_string()],
            kind: "zsh".to_string(),
        }],
        plugins: vec![ImportedPlugin {
            name: "git".to_string(),
            source_dir: None,
            plugin_script: None,
            completion_files: Vec::new(),
            alias_count: 1,
            diagnostics_count: 0,
            tier: PluginImportTier::Tier1Safe,
            import_kind: PluginImportKind::AliasAndCompletion,
            capabilities: Vec::new(),
            unsupported_features: Vec::new(),
        }],
        oh_my_zsh_detected: true,
        edit_mode: Some("vi".to_string()),
        diagnostics: vec![
            diagnostic(DiagnosticSeverity::Info, "info"),
            diagnostic(DiagnosticSeverity::Warn, "warn"),
            diagnostic(DiagnosticSeverity::Unsupported, "unsupported"),
        ],
        ..Default::default()
    };
    let status = ZshImportConfigStatus {
        config_path: config_path.clone(),
        config_exists: true,
        block_state: ZshImportBlockState::Missing,
        toml_valid: true,
        toml_error: None,
        apply_readiness: ZshImportApplyReadiness::AddNewBlock,
        apply_error: None,
        backup_paths: Vec::new(),
    };
    let rollback = ZshImportRollbackPlan {
        config_path,
        backup_paths: vec![backup_path.clone()],
        latest_backup_path: Some(backup_path),
        restore_command: Some("Copy-Item -LiteralPath 'backup' -Destination 'config' -Force".to_string()),
    };

    let doctor = zsh_compat_doctor_text(&report, &status, &rollback);

    assert!(doctor.contains("Zsh compatibility doctor"));
    assert!(doctor.contains("Discovered: source files=1, oh-my-zsh=yes"));
    assert!(doctor.contains("Imports: aliases=1, env=1, PATH entries=1, completions=1, plugins=1"));
    assert!(doctor.contains("Plugin tiers: safe=1, partial=0, native=0, unsupported=0, missing=0"));
    assert!(doctor.contains("Native UX: edit_mode=vi"));
    assert!(doctor.contains("Diagnostics: info=1, warnings=1, unsupported=1"));
    assert!(doctor.contains("Next apply: ready (add new block)"));
    assert!(doctor.contains("Apply safe import: winuxsh --zsh-compat-import-apply"));
    assert!(doctor.contains("Restore latest backup: Copy-Item"));
}

#[test]
fn formats_doctor_summary_for_blocked_import() {
    let config_path = PathBuf::from("C:/Users/me/.winshrc.toml");
    let report = ZshImportReport::default();
    let status = ZshImportConfigStatus {
        config_path: config_path.clone(),
        config_exists: true,
        block_state: ZshImportBlockState::Malformed,
        toml_valid: true,
        toml_error: None,
        apply_readiness: ZshImportApplyReadiness::Blocked,
        apply_error: Some("found malformed marker".to_string()),
        backup_paths: Vec::new(),
    };
    let rollback = ZshImportRollbackPlan {
        config_path,
        backup_paths: Vec::new(),
        latest_backup_path: None,
        restore_command: None,
    };

    let doctor = zsh_compat_doctor_text(&report, &status, &rollback);

    assert!(doctor.contains("managed_block=malformed"));
    assert!(doctor.contains("Next apply: blocked"));
    assert!(doctor.contains("Apply detail: found malformed marker"));
    assert!(doctor.contains("Resolve blocker or merge manually"));
    assert!(!doctor.contains("Apply safe import: winuxsh --zsh-compat-import-apply"));
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

#[test]
fn translates_dynamic_zsh_completion_generator_output_with_runner() {
    let report = ZshImportReport {
        dynamic_completion_sources: vec![DynamicCompletionSource {
            command: "kubectl".to_string(),
            args: vec!["completion".to_string(), "zsh".to_string()],
            target_shell: "zsh".to_string(),
            source_file: None,
            line: None,
            origin: "plugin".to_string(),
        }],
        ..Default::default()
    };

    let defs = dynamic_completion_defs_from_report_with_runner(&report, |source| {
        assert_eq!(source.command, "kubectl");
        assert_eq!(source.args, vec!["completion", "zsh"]);
        Ok(
            r#"
#compdef kubectl
_arguments \
  "--namespace=[namespace scope]::namespace:_files" \
  "--all-namespaces[all namespaces]" \
  "-o[output format]:format:(json yaml wide)"
"#
            .to_string(),
        )
    });

    assert_eq!(defs.len(), 1);
    let kubectl = &defs[0];
    assert_eq!(kubectl.command, "kubectl");
    assert!(kubectl.flags.iter().any(|flag| {
        flag.long.as_deref() == Some("--namespace") && flag.takes_value
    }));
    assert!(kubectl.flags.iter().any(|flag| {
        flag.long.as_deref() == Some("--all-namespaces") && !flag.takes_value
    }));
    assert!(kubectl
        .flags
        .iter()
        .any(|flag| flag.short.as_deref() == Some("-o") && flag.takes_value));
}

#[test]
fn runs_allowed_dynamic_completion_generator_with_timeout() {
    let _lock = env_lock().lock().unwrap();
    let _env = EnvGuard::capture(&["PATH"]);
    let temp = unique_temp_dir("winuxsh-zsh-dynamic-completion-runner");
    std::fs::create_dir_all(&temp).unwrap();
    let command_path = temp.join("dyncli.cmd");
    std::fs::write(
        &command_path,
        r#"@echo off
if "%1"=="completion" if "%2"=="zsh" goto completion
exit /b 2
:completion
echo #compdef dyncli.cmd
echo _arguments \
echo   "--config=[config file]::config:_files" \
echo   "--verbose[verbose output]"
"#,
    )
    .unwrap();

    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path_entries = vec![temp.clone()];
    path_entries.extend(std::env::split_paths(&old_path));
    std::env::set_var("PATH", std::env::join_paths(path_entries).unwrap());

    let report = ZshImportReport {
        dynamic_completion_sources: vec![DynamicCompletionSource {
            command: "dyncli.cmd".to_string(),
            args: vec!["completion".to_string(), "zsh".to_string()],
            target_shell: "zsh".to_string(),
            source_file: None,
            line: None,
            origin: "plugin".to_string(),
        }],
        ..Default::default()
    };

    assert!(dynamic_completion_defs_from_report_with_options(
        &report,
        &DynamicCompletionRunOptions::default()
    )
    .is_empty());

    let options = DynamicCompletionRunOptions {
        allowed_commands: vec!["dyncli.cmd".to_string()],
        timeout: Duration::from_secs(2),
    };
    let defs = dynamic_completion_defs_from_report_with_options(&report, &options);

    assert_eq!(defs.len(), 1);
    let dyncli = &defs[0];
    assert_eq!(dyncli.command, "dyncli.cmd");
    assert!(dyncli.flags.iter().any(|flag| {
        flag.long.as_deref() == Some("--config") && flag.takes_value
    }));
    assert!(dyncli.flags.iter().any(|flag| {
        flag.long.as_deref() == Some("--verbose") && !flag.takes_value
    }));

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

fn diagnostic(severity: DiagnosticSeverity, feature: &str) -> ZshCompatDiagnostic {
    ZshCompatDiagnostic {
        severity,
        feature: feature.to_string(),
        message: format!("{} message", feature),
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
