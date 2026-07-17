//! Safe zsh / Oh My Zsh compatibility scanner.
//!
//! This module reads zsh-style config and plugin assets, but never executes
//! zsh scripts. It produces a report that can be shown to users or later
//! applied through explicit, safe winuxsh hooks.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::completion::external::{CommandDef, FlagDef, PathLiteral, ValuesSource};
use crate::config::{EditorMode, ZshCompatLevel, ZshConfig};

#[derive(Debug, Clone)]
pub struct ZshImportOptions {
    pub enabled: bool,
    pub zdotdir: PathBuf,
    pub import_zshrc: bool,
    pub import_oh_my_zsh: bool,
    pub plugins: Vec<String>,
    pub compat_level: ZshCompatLevel,
}

impl ZshImportOptions {
    pub fn from_config(config: &ZshConfig) -> Self {
        Self {
            enabled: config.enabled,
            zdotdir: config.zdotdir.clone().unwrap_or_else(default_zdotdir),
            import_zshrc: config.import_zshrc,
            import_oh_my_zsh: config.import_oh_my_zsh,
            plugins: config.plugins.clone(),
            compat_level: config.compat_level,
        }
    }

    pub fn for_report(config: &ZshConfig) -> Self {
        let mut options = Self::from_config(config);
        options.enabled = true;
        options
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ZshImportReport {
    pub source_files: Vec<PathBuf>,
    pub aliases: Vec<ImportedAlias>,
    pub env: Vec<ImportedEnv>,
    pub path_entries: Vec<PathBuf>,
    pub fpath_entries: Vec<PathBuf>,
    pub plugins: Vec<ImportedPlugin>,
    pub theme: Option<String>,
    pub edit_mode: Option<String>,
    pub zstyles: Vec<ImportedZstyle>,
    pub completion_assets: Vec<CompletionAsset>,
    pub oh_my_zsh_detected: bool,
    pub diagnostics: Vec<ZshCompatDiagnostic>,
}

impl ZshImportReport {
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_human(&self) -> String {
        let mut out = Vec::new();
        out.push("Zsh compatibility report".to_string());
        out.push(format!("source files: {}", self.source_files.len()));
        for source in &self.source_files {
            out.push(format!("  - {}", source.display()));
        }
        out.push(format!("aliases: {}", self.aliases.len()));
        out.push(format!("env assignments: {}", self.env.len()));
        out.push(format!("PATH entries: {}", self.path_entries.len()));
        out.push(format!("fpath entries: {}", self.fpath_entries.len()));
        out.push(format!("plugins: {}", self.plugins.len()));
        out.push(format!("completion assets: {}", self.completion_assets.len()));
        out.push(format!("zstyles: {}", self.zstyles.len()));
        out.push(format!(
            "theme: {}",
            self.theme.as_deref().unwrap_or("(none)")
        ));
        out.push(format!(
            "edit mode: {}",
            self.edit_mode.as_deref().unwrap_or("(none)")
        ));
        out.push(format!("Oh My Zsh detected: {}", self.oh_my_zsh_detected));

        if !self.plugins.is_empty() {
            out.push("plugins detail:".to_string());
            for plugin in &self.plugins {
                out.push(format!(
                    "  - {} kind={:?} tier={:?} aliases={} completions={} dir={}",
                    plugin.name,
                    plugin.import_kind,
                    plugin.tier,
                    plugin.alias_count,
                    plugin.completion_files.len(),
                    plugin
                        .source_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "(not found)".to_string())
                ));
            }
        }

        if !self.diagnostics.is_empty() {
            out.push("diagnostics:".to_string());
            for diag in &self.diagnostics {
                let source = diag
                    .source_file
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(unknown)".to_string());
                let line = diag.line.map_or(String::new(), |line| format!(":{}", line));
                out.push(format!(
                    "  - [{:?}] {}{} {}: {}",
                    diag.severity, source, line, diag.feature, diag.message
                ));
            }
        }

        out.join("\n")
    }
}

#[derive(Debug, Clone, Default)]
pub struct SafeApplySummary {
    pub env_applied: usize,
    pub aliases_applied: usize,
    pub path_entries_applied: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedAlias {
    pub name: String,
    pub value: String,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedEnv {
    pub key: String,
    pub value: String,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedPlugin {
    pub name: String,
    pub source_dir: Option<PathBuf>,
    pub plugin_script: Option<PathBuf>,
    pub completion_files: Vec<PathBuf>,
    pub alias_count: usize,
    pub diagnostics_count: usize,
    pub tier: PluginImportTier,
    pub import_kind: PluginImportKind,
    pub capabilities: Vec<String>,
    pub unsupported_features: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginImportTier {
    Tier1Safe,
    Tier2Partial,
    Tier3Native,
    Tier4Unsupported,
    Missing,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginImportKind {
    CompletionOnly,
    AliasOnly,
    AliasAndCompletion,
    NativeUx,
    Partial,
    Unsupported,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedZstyle {
    pub context: String,
    pub key: String,
    pub values: Vec<String>,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionAsset {
    pub source_file: PathBuf,
    pub commands: Vec<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZshCompatDiagnostic {
    pub severity: DiagnosticSeverity,
    pub feature: String,
    pub message: String,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warn,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanMode {
    Profile,
    Plugin,
}

pub fn scan(options: &ZshImportOptions) -> ZshImportReport {
    let mut report = ZshImportReport::default();

    if !options.enabled {
        report.diagnostics.push(ZshCompatDiagnostic {
            severity: DiagnosticSeverity::Info,
            feature: "zsh.enabled".to_string(),
            message: "zsh compatibility is disabled".to_string(),
            source_file: None,
            line: None,
        });
        return report;
    }

    let mut env_map = base_env_map(options);

    if options.import_zshrc {
        let zshrc = options.zdotdir.join(".zshrc");
        scan_profile_file(&zshrc, &mut report, &mut env_map, ScanMode::Profile);
    }

    if options.import_oh_my_zsh {
        scan_oh_my_zsh_layout(options, &mut report, &mut env_map);
    }

    report
}

pub fn apply_safe_env(report: &ZshImportReport) -> SafeApplySummary {
    let mut summary = SafeApplySummary::default();

    if let Some(path) = safe_path_value(report) {
        std::env::set_var("PATH", path);
        summary.path_entries_applied = report.path_entries.len();
        summary.env_applied += 1;
    }

    for env in &report.env {
        if is_safe_env_key(&env.key) && env.key != "PATH" {
            std::env::set_var(&env.key, &env.value);
            summary.env_applied += 1;
        }
    }

    summary
}

pub fn apply_safe_aliases(
    report: &ZshImportReport,
    executor: &mut rubash::executor::Executor,
) -> SafeApplySummary {
    let mut summary = SafeApplySummary::default();

    for alias in &report.aliases {
        if apply_alias(executor, &alias.name, &alias.value) {
            summary.aliases_applied += 1;
        }
    }

    summary
}

pub fn apply_alias(
    executor: &mut rubash::executor::Executor,
    name: &str,
    value: &str,
) -> bool {
    if !is_identifierish(name) {
        return false;
    }

    let source = format!("alias {}={}", name, shell_quote(value));
    let tokens = rubash::lexer::tokenize(&source);
    if tokens.is_empty() {
        return false;
    }
    let ast = rubash::parser::parse(&tokens);
    executor.execute_ast(&ast).is_ok() && executor.last_exit_code() == 0
}

pub fn safe_path_value(report: &ZshImportReport) -> Option<OsString> {
    if report.path_entries.is_empty() {
        return None;
    }

    let mut seen = HashSet::new();
    let mut parts: Vec<PathBuf> = Vec::new();
    for entry in &report.path_entries {
        if entry.as_os_str().is_empty() {
            continue;
        }
        let key = normalise_path_key(entry);
        if seen.insert(key) {
            parts.push(entry.clone());
        }
    }

    for entry in current_path_entries() {
        let key = normalise_path_key(&entry);
        if seen.insert(key) {
            parts.push(entry);
        }
    }

    std::env::join_paths(parts).ok()
}

pub fn completion_defs_from_report(report: &ZshImportReport) -> Vec<CommandDef> {
    let mut definitions: HashMap<String, CommandDef> = HashMap::new();

    for asset in &report.completion_assets {
        let flags = parse_zsh_completion_flags(&asset.source_file);
        for command in &asset.commands {
            if !is_safe_name(command) {
                continue;
            }
            let def = definitions
                .entry(command.clone())
                .or_insert_with(|| CommandDef {
                    command: command.clone(),
                    description: Some(format!(
                        "Imported from zsh completion asset {}",
                        asset.source_file.display()
                    )),
                    flags: Vec::new(),
                    subcommands: Vec::new(),
                });
            merge_flags(&mut def.flags, flags.clone());
        }
    }

    let mut values: Vec<CommandDef> = definitions.into_values().collect();
    values.sort_by(|left, right| left.command.cmp(&right.command));
    values
}

fn default_zdotdir() -> PathBuf {
    std::env::var_os("ZDOTDIR")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn base_env_map(options: &ZshImportOptions) -> HashMap<String, String> {
    let mut env = std::env::vars().collect::<HashMap<_, _>>();
    if let Some(home) = dirs::home_dir() {
        env.entry("HOME".to_string())
            .or_insert_with(|| home.to_string_lossy().to_string());
    }
    env.insert(
        "ZDOTDIR".to_string(),
        options.zdotdir.to_string_lossy().to_string(),
    );
    env
}

fn scan_profile_file(
    path: &Path,
    report: &mut ZshImportReport,
    env_map: &mut HashMap<String, String>,
    mode: ScanMode,
) {
    if !path.is_file() {
        report.diagnostics.push(ZshCompatDiagnostic {
            severity: DiagnosticSeverity::Info,
            feature: "profile".to_string(),
            message: format!("profile file not found: {}", path.display()),
            source_file: Some(path.to_path_buf()),
            line: None,
        });
        return;
    }

    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            report.diagnostics.push(ZshCompatDiagnostic {
                severity: DiagnosticSeverity::Warn,
                feature: "profile".to_string(),
                message: format!("failed to read profile: {}", err),
                source_file: Some(path.to_path_buf()),
                line: None,
            });
            return;
        }
    };

    report.source_files.push(path.to_path_buf());
    scan_content(&content, Some(path), report, env_map, mode);
}

fn scan_content(
    content: &str,
    source_file: Option<&Path>,
    report: &mut ZshImportReport,
    env_map: &mut HashMap<String, String>,
    mode: ScanMode,
) {
    for (line_no, logical) in logical_lines(content) {
        let Some(line) = strip_inline_comment(&logical) else {
            continue;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        scan_unsupported(line, source_file, line_no, report);

        if let Some((name, value)) = parse_alias(line, source_file, line_no, report) {
            report.aliases.push(ImportedAlias {
                name,
                value,
                source_file: source_file.map(Path::to_path_buf),
                line: Some(line_no),
                origin: match mode {
                    ScanMode::Profile => "profile",
                    ScanMode::Plugin => "plugin",
                }
                .to_string(),
            });
            continue;
        }

        if let Some((context, key, values)) = parse_zstyle(line) {
            report.zstyles.push(ImportedZstyle {
                context,
                key,
                values,
                source_file: source_file.map(Path::to_path_buf),
                line: Some(line_no),
            });
            continue;
        }

        if let Some(commands) = parse_compdef_line(line) {
            if let Some(source) = source_file {
                push_completion_asset(
                    report,
                    CompletionAsset {
                        source_file: source.to_path_buf(),
                        commands,
                        kind: "compdef".to_string(),
                    },
                );
            }
            continue;
        }

        if mode == ScanMode::Plugin {
            continue;
        }

        if let Some(values) = parse_named_array(line, "plugins") {
            add_plugins(report, values);
            continue;
        }

        if let Some(values) = parse_named_array(line, "path") {
            for value in values {
                add_path_entry(report, env_map, &value, true);
            }
            continue;
        }

        if let Some(values) = parse_named_array(line, "fpath") {
            for value in values {
                add_fpath_entry(report, env_map, &value);
            }
            continue;
        }

        if is_omz_source_line(line) {
            report.oh_my_zsh_detected = true;
            report.diagnostics.push(ZshCompatDiagnostic {
                severity: DiagnosticSeverity::Info,
                feature: "source".to_string(),
                message: "Oh My Zsh loader detected; scanner will inspect layout instead of sourcing it".to_string(),
                source_file: source_file.map(Path::to_path_buf),
                line: Some(line_no),
            });
            continue;
        }

        if let Some((key, value)) = parse_assignment(line) {
            record_assignment(key, value, source_file, line_no, report, env_map);
        }
    }
}

fn scan_oh_my_zsh_layout(
    options: &ZshImportOptions,
    report: &mut ZshImportReport,
    env_map: &mut HashMap<String, String>,
) {
    let zsh_dir = env_map
        .get("ZSH")
        .map(PathBuf::from)
        .unwrap_or_else(|| options.zdotdir.join(".oh-my-zsh"));
    let zsh_custom = env_map
        .get("ZSH_CUSTOM")
        .map(PathBuf::from)
        .unwrap_or_else(|| zsh_dir.join("custom"));

    let plugin_names = merged_plugin_names(report, &options.plugins);
    report.plugins.clear();
    for plugin_name in plugin_names {
        if !is_safe_name(&plugin_name) {
            report.diagnostics.push(ZshCompatDiagnostic {
                severity: DiagnosticSeverity::Unsupported,
                feature: "plugin".to_string(),
                message: format!("unsafe plugin name skipped: {}", plugin_name),
                source_file: None,
                line: None,
            });
            continue;
        }

        let source_dir = [zsh_custom.join("plugins").join(&plugin_name), zsh_dir.join("plugins").join(&plugin_name)]
            .into_iter()
            .find(|path| path.is_dir());

        let Some(source_dir) = source_dir else {
            report.plugins.push(unresolved_plugin(plugin_name, 1));
            continue;
        };

        let alias_before = report.aliases.len();
        let diagnostics_before = report.diagnostics.len();
        let plugin_script = source_dir.join(format!("{}.plugin.zsh", plugin_name));
        let plugin_script = if plugin_script.is_file() {
            scan_profile_file(&plugin_script, report, env_map, ScanMode::Plugin);
            Some(plugin_script)
        } else {
            None
        };

        let completion_files = collect_completion_files(&source_dir);
        for file in &completion_files {
            if let Ok(content) = std::fs::read_to_string(file) {
                for (line_no, line) in content.lines().enumerate().take(20) {
                    if let Some(commands) = parse_compdef_line(line.trim()) {
                        push_completion_asset(
                            report,
                            CompletionAsset {
                                source_file: file.clone(),
                                commands,
                                kind: "#compdef".to_string(),
                            },
                        );
                    }
                    if line_no > 0 && !line.trim().is_empty() && !line.trim().starts_with('#') {
                        break;
                    }
                }
            }
        }

        let alias_count = report.aliases.len().saturating_sub(alias_before);
        let diagnostics_count = report.diagnostics.len().saturating_sub(diagnostics_before);
        let unsupported_features =
            unsupported_features_since(&report.diagnostics, diagnostics_before);
        let (tier, import_kind, capabilities) = classify_plugin(
            &plugin_name,
            alias_count,
            !completion_files.is_empty(),
            plugin_script.is_some(),
            &unsupported_features,
        );

        report.plugins.push(ImportedPlugin {
            name: plugin_name,
            source_dir: Some(source_dir),
            plugin_script,
            completion_files,
            alias_count,
            diagnostics_count,
            tier,
            import_kind,
            capabilities,
            unsupported_features,
        });
    }
}

fn unresolved_plugin(name: String, diagnostics_count: usize) -> ImportedPlugin {
    ImportedPlugin {
        name,
        source_dir: None,
        plugin_script: None,
        completion_files: Vec::new(),
        alias_count: 0,
        diagnostics_count,
        tier: PluginImportTier::Missing,
        import_kind: PluginImportKind::Missing,
        capabilities: Vec::new(),
        unsupported_features: Vec::new(),
    }
}

fn classify_plugin(
    name: &str,
    alias_count: usize,
    has_completion: bool,
    has_script: bool,
    unsupported_features: &[String],
) -> (PluginImportTier, PluginImportKind, Vec<String>) {
    let has_aliases = alias_count > 0;
    let has_safe_assets = has_aliases || has_completion;
    let has_unsupported = !unsupported_features.is_empty();
    let native_ux = is_native_ux_plugin(name)
        || unsupported_features
            .iter()
            .any(|feature| matches!(feature.as_str(), "zle-buffer" | "zle-highlighting"));

    let mut capabilities = Vec::new();
    if has_aliases {
        capabilities.push("aliases".to_string());
    }
    if has_completion {
        capabilities.push("static_completions".to_string());
    }
    if native_ux {
        capabilities.push("native_ux_required".to_string());
    }
    if has_unsupported {
        capabilities.push("unsupported_zsh_internals".to_string());
    }
    if has_script && !has_safe_assets && !has_unsupported {
        capabilities.push("script_unclassified".to_string());
    }

    if native_ux {
        return (PluginImportTier::Tier3Native, PluginImportKind::NativeUx, capabilities);
    }
    if has_unsupported && has_safe_assets {
        return (PluginImportTier::Tier2Partial, PluginImportKind::Partial, capabilities);
    }
    if has_unsupported {
        return (
            PluginImportTier::Tier4Unsupported,
            PluginImportKind::Unsupported,
            capabilities,
        );
    }

    match (has_aliases, has_completion) {
        (true, true) => (
            PluginImportTier::Tier1Safe,
            PluginImportKind::AliasAndCompletion,
            capabilities,
        ),
        (true, false) => (
            PluginImportTier::Tier1Safe,
            PluginImportKind::AliasOnly,
            capabilities,
        ),
        (false, true) => (
            PluginImportTier::Tier1Safe,
            PluginImportKind::CompletionOnly,
            capabilities,
        ),
        (false, false) => (
            PluginImportTier::Tier2Partial,
            PluginImportKind::Partial,
            capabilities,
        ),
    }
}

fn unsupported_features_since(
    diagnostics: &[ZshCompatDiagnostic],
    start_index: usize,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut features = Vec::new();
    for diagnostic in diagnostics.iter().skip(start_index) {
        if diagnostic.severity == DiagnosticSeverity::Unsupported
            && seen.insert(diagnostic.feature.clone())
        {
            features.push(diagnostic.feature.clone());
        }
    }
    features
}

fn is_native_ux_plugin(name: &str) -> bool {
    matches!(
        name,
        "zsh-autosuggestions"
            | "zsh-syntax-highlighting"
            | "fast-syntax-highlighting"
            | "zsh-history-substring-search"
            | "fzf-tab"
    )
}

fn logical_lines(content: &str) -> Vec<(usize, String)> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut start_line = 1;
    let mut paren_depth = 0isize;

    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx + 1;
        let mut line = raw.trim_end_matches('\r').to_string();
        let continued = line.ends_with('\\');
        if continued {
            line.pop();
        }

        if current.is_empty() {
            start_line = line_no;
        } else {
            current.push(' ');
        }
        paren_depth += paren_delta(&line);
        current.push_str(line.trim());

        if continued || paren_depth > 0 {
            continue;
        }

        result.push((start_line, current.trim().to_string()));
        current.clear();
        paren_depth = 0;
    }

    if !current.trim().is_empty() {
        result.push((start_line, current.trim().to_string()));
    }

    result
}

fn paren_delta(line: &str) -> isize {
    let mut single = false;
    let mut double = false;
    let mut delta = 0;
    let mut prev_escape = false;
    for ch in line.chars() {
        if prev_escape {
            prev_escape = false;
            continue;
        }
        if ch == '\\' {
            prev_escape = true;
            continue;
        }
        match ch {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '(' if !single && !double => delta += 1,
            ')' if !single && !double => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn strip_inline_comment(line: &str) -> Option<String> {
    let mut single = false;
    let mut double = false;
    let mut prev = '\0';
    let mut out = String::new();
    for ch in line.chars() {
        match ch {
            '\'' if !double => {
                single = !single;
                out.push(ch);
            }
            '"' if !single => {
                double = !double;
                out.push(ch);
            }
            '#' if !single && !double && (prev == '\0' || prev.is_whitespace()) => break,
            _ => out.push(ch),
        }
        prev = ch;
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_alias(
    line: &str,
    source_file: Option<&Path>,
    line_no: usize,
    report: &mut ZshImportReport,
) -> Option<(String, String)> {
    let rest = line.strip_prefix("alias ")?;
    let rest = rest.trim();
    if rest.starts_with("-g ") || rest.starts_with("-s ") {
        report.diagnostics.push(ZshCompatDiagnostic {
            severity: DiagnosticSeverity::Unsupported,
            feature: "alias".to_string(),
            message: "global and suffix aliases are not imported".to_string(),
            source_file: source_file.map(Path::to_path_buf),
            line: Some(line_no),
        });
        return None;
    }

    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    if !is_identifierish(name) {
        return None;
    }
    Some((name.to_string(), unquote(value.trim())))
}

fn parse_zstyle(line: &str) -> Option<(String, String, Vec<String>)> {
    let rest = line.strip_prefix("zstyle ")?;
    let words = split_shell_words(rest);
    if words.len() < 2 {
        return None;
    }
    Some((words[0].clone(), words[1].clone(), words[2..].to_vec()))
}

fn parse_compdef_line(line: &str) -> Option<Vec<String>> {
    let rest = if let Some(rest) = line.strip_prefix("#compdef ") {
        rest
    } else if let Some(rest) = line.strip_prefix("compdef ") {
        rest
    } else {
        return None;
    };

    let words = split_shell_words(rest);
    let mut commands = Vec::new();
    for (idx, word) in words.into_iter().enumerate() {
        if idx == 0 && word.starts_with('_') {
            continue;
        }
        if word.starts_with('-') {
            continue;
        }
        let command = word.split_once('=').map(|(left, _)| left).unwrap_or(&word);
        if is_safe_name(command) {
            commands.push(command.to_string());
        }
    }
    if commands.is_empty() {
        None
    } else {
        Some(commands)
    }
}

fn parse_named_array(line: &str, expected: &str) -> Option<Vec<String>> {
    let (key, value) = line.split_once('=')?;
    if key.trim() != expected {
        return None;
    }
    let value = value.trim();
    let inner = value.strip_prefix('(')?.strip_suffix(')')?;
    Some(split_shell_words(inner))
}

fn parse_assignment(line: &str) -> Option<(String, String)> {
    let line = line.strip_prefix("export ").unwrap_or(line).trim();
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if !is_identifierish(key) {
        return None;
    }
    Some((key.to_string(), unquote(value.trim())))
}

fn record_assignment(
    key: String,
    value: String,
    source_file: Option<&Path>,
    line_no: usize,
    report: &mut ZshImportReport,
    env_map: &mut HashMap<String, String>,
) {
    let expanded = expand_value(&value, env_map);
    match key.as_str() {
        "ZSH_THEME" => {
            report.theme = Some(expanded.clone());
        }
        "PATH" => {
            for entry in split_path_like(&value) {
                add_path_entry(report, env_map, entry, true);
            }
        }
        "fpath" => {
            for entry in split_path_like(&value) {
                add_fpath_entry(report, env_map, entry);
            }
        }
        "ZDOTDIR" | "ZSH" | "ZSH_CUSTOM" | "ZSH_CACHE_DIR" | "CASE_SENSITIVE"
        | "HYPHEN_INSENSITIVE" | "ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE"
        | "ZSH_AUTOSUGGEST_STRATEGY" | "ZSH_AUTOSUGGEST_BUFFER_MAX_SIZE"
        | "ZSH_HIGHLIGHT_STYLES" | "ZSH_HIGHLIGHT_HIGHLIGHTERS" => {
            env_map.insert(key.clone(), expanded.clone());
        }
        _ => {}
    }

    report.env.push(ImportedEnv {
        key,
        value: expanded,
        source_file: source_file.map(Path::to_path_buf),
        line: Some(line_no),
    });
}

fn add_plugins(report: &mut ZshImportReport, plugins: Vec<String>) {
    let mut seen: HashSet<String> = report.plugins.iter().map(|p| p.name.clone()).collect();
    for name in plugins {
        if name.starts_with('$') || name.is_empty() {
            continue;
        }
        if seen.insert(name.clone()) {
            report.plugins.push(unresolved_plugin(name, 0));
        }
    }
}

fn add_path_entry(
    report: &mut ZshImportReport,
    env_map: &HashMap<String, String>,
    value: &str,
    skip_existing_var: bool,
) {
    let value = value.trim();
    if value.is_empty() || (skip_existing_var && is_path_var_ref(value)) {
        return;
    }
    report.path_entries.push(PathBuf::from(expand_value(value, env_map)));
}

fn add_fpath_entry(report: &mut ZshImportReport, env_map: &HashMap<String, String>, value: &str) {
    let value = value.trim();
    if value.is_empty() || value == "$fpath" || value == "${fpath}" {
        return;
    }
    report.fpath_entries.push(PathBuf::from(expand_value(value, env_map)));
}

fn is_omz_source_line(line: &str) -> bool {
    (line.starts_with("source ") || line.starts_with(". "))
        && line.contains("oh-my-zsh.sh")
}

fn scan_unsupported(
    line: &str,
    source_file: Option<&Path>,
    line_no: usize,
    report: &mut ZshImportReport,
) {
    for (needle, feature, message) in [
        ("zle ", "zle", "ZLE widgets require native reedline implementation"),
        ("zle\t", "zle", "ZLE widgets require native reedline implementation"),
        ("zmodload", "zmodload", "zsh modules are not available in winuxsh"),
        ("zpty", "zpty", "zpty-backed plugins require a real zsh interpreter"),
        ("BUFFER", "zle-buffer", "BUFFER/CURSOR style plugins are not executed"),
        ("CURSOR", "zle-buffer", "BUFFER/CURSOR style plugins are not executed"),
        (
            "region_highlight",
            "zle-highlighting",
            "region_highlight maps to native reedline highlighting",
        ),
    ] {
        if line.contains(needle) {
            report.diagnostics.push(ZshCompatDiagnostic {
                severity: DiagnosticSeverity::Unsupported,
                feature: feature.to_string(),
                message: message.to_string(),
                source_file: source_file.map(Path::to_path_buf),
                line: Some(line_no),
            });
        }
    }

    if line == "bindkey -e" {
        report.edit_mode = Some("emacs".to_string());
    } else if line == "bindkey -v" {
        report.edit_mode = Some("vi".to_string());
    } else if line.starts_with("bindkey ") {
        report.diagnostics.push(ZshCompatDiagnostic {
            severity: DiagnosticSeverity::Unsupported,
            feature: "bindkey".to_string(),
            message: "custom bindkey mappings are not imported yet".to_string(),
            source_file: source_file.map(Path::to_path_buf),
            line: Some(line_no),
        });
    }
}

fn merged_plugin_names(report: &ZshImportReport, configured: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for name in report
        .plugins
        .iter()
        .map(|plugin| plugin.name.as_str())
        .chain(configured.iter().map(String::as_str))
    {
        if seen.insert(name.to_string()) {
            names.push(name.to_string());
        }
    }
    names
}

fn collect_completion_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_completion_files_inner(dir, &mut out);
    out.sort();
    out
}

fn collect_completion_files_inner(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_completion_files_inner(&path, out);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with('_'))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

fn push_completion_asset(report: &mut ZshImportReport, asset: CompletionAsset) {
    if report.completion_assets.iter().any(|existing| {
        existing.source_file == asset.source_file
            && existing.commands == asset.commands
            && existing.kind == asset.kind
    }) {
        return;
    }
    report.completion_assets.push(asset);
}

fn parse_zsh_completion_flags(path: &Path) -> Vec<FlagDef> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_zsh_argument_flags(&content)
}

fn parse_zsh_argument_flags(content: &str) -> Vec<FlagDef> {
    let mut flags = Vec::new();

    for (_, logical) in logical_lines(content) {
        let Some(line) = strip_inline_comment(&logical) else {
            continue;
        };
        let Some((_, rest)) = line.split_once("_arguments") else {
            continue;
        };

        let mut seen_spec = false;
        for word in split_shell_words(rest) {
            if !seen_spec && is_arguments_control_word(&word) {
                continue;
            }
            if let Some(flag) = parse_zsh_argument_flag(&word) {
                seen_spec = true;
                merge_flags(&mut flags, vec![flag]);
            }
        }
    }

    flags
}

fn is_arguments_control_word(word: &str) -> bool {
    matches!(
        word,
        "-0" | "-A"
            | "-C"
            | "-M"
            | "-O"
            | "-R"
            | "-S"
            | "-W"
            | "-a"
            | "-c"
            | "-e"
            | "-i"
            | "-n"
            | "-s"
            | "-w"
    )
}

fn parse_zsh_argument_flag(word: &str) -> Option<FlagDef> {
    let mut spec = word.trim();
    if spec.is_empty()
        || spec.starts_with(':')
        || spec.starts_with('*')
        || spec.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    {
        return None;
    }

    while spec.starts_with('(') {
        let close = matching_close_paren(spec)?;
        spec = spec[close + 1..].trim_start();
    }

    let (flag_part, description, suffix) = split_zsh_description(spec);
    let flag_part = flag_part.trim();
    if flag_part.is_empty() {
        return None;
    }

    let candidates = flag_candidates(flag_part);
    if candidates.is_empty() {
        return None;
    }

    let short = candidates
        .iter()
        .find(|candidate| is_short_flag(candidate))
        .cloned();
    let long = candidates
        .iter()
        .find(|candidate| is_long_flag(candidate))
        .cloned();

    if short.is_none() && long.is_none() {
        return None;
    }

    let takes_value = zsh_spec_takes_value(suffix);
    let values_source = if takes_value && suffix.contains("_files") {
        Some(ValuesSource::Path {
            values_from: PathLiteral,
        })
    } else {
        None
    };

    Some(FlagDef {
        short,
        long,
        description,
        takes_value,
        values_source,
    })
}

fn split_zsh_description(spec: &str) -> (&str, Option<String>, &str) {
    let Some(open) = spec.find('[') else {
        let flag_part = spec.split(':').next().unwrap_or(spec);
        let suffix = spec.strip_prefix(flag_part).unwrap_or_default();
        return (flag_part, None, suffix);
    };

    let Some(close_rel) = spec[open + 1..].find(']') else {
        let flag_part = spec.split(':').next().unwrap_or(spec);
        let suffix = spec.strip_prefix(flag_part).unwrap_or_default();
        return (flag_part, None, suffix);
    };

    let close = open + 1 + close_rel;
    let desc = spec[open + 1..close].trim();
    let suffix = &spec[close + 1..];
    (
        &spec[..open],
        (!desc.is_empty()).then(|| desc.to_string()),
        suffix,
    )
}

fn flag_candidates(flag_part: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    if let Some((inside, _)) = braced_candidates(flag_part) {
        for part in inside.split(',') {
            push_flag_candidate(&mut candidates, part);
        }
        return candidates;
    }

    for part in flag_part.split(|ch: char| ch == ',' || ch.is_whitespace()) {
        push_flag_candidate(&mut candidates, part);
    }

    candidates
}

fn braced_candidates(value: &str) -> Option<(&str, &str)> {
    let open = value.find('{')?;
    let close = value[open + 1..].find('}')? + open + 1;
    Some((&value[open + 1..close], &value[close + 1..]))
}

fn push_flag_candidate(candidates: &mut Vec<String>, raw: &str) {
    let candidate = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .split_once('=')
        .map(|(left, _)| left)
        .unwrap_or(raw)
        .split_once(':')
        .map(|(left, _)| left)
        .unwrap_or(raw)
        .trim();

    if (is_short_flag(candidate) || is_long_flag(candidate))
        && !candidates.iter().any(|existing| existing == candidate)
    {
        candidates.push(candidate.to_string());
    }
}

fn zsh_spec_takes_value(suffix: &str) -> bool {
    suffix.contains("::")
        || suffix.contains(":_")
        || suffix.contains(":->")
        || suffix.matches(':').count() >= 2
}

fn is_short_flag(value: &str) -> bool {
    value.starts_with('-') && !value.starts_with("--") && value.len() == 2
}

fn is_long_flag(value: &str) -> bool {
    value.starts_with("--") && value.len() > 2
}

fn matching_close_paren(value: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn merge_flags(target: &mut Vec<FlagDef>, incoming: Vec<FlagDef>) {
    for flag in incoming {
        if let Some(existing) = target.iter_mut().find(|existing| same_flag(existing, &flag)) {
            if existing.short.is_none() {
                existing.short = flag.short.clone();
            }
            if existing.long.is_none() {
                existing.long = flag.long.clone();
            }
            if existing.description.is_none() {
                existing.description = flag.description.clone();
            }
            if !existing.takes_value {
                existing.takes_value = flag.takes_value;
            }
            if existing.values_source.is_none() {
                existing.values_source = flag.values_source.clone();
            }
        } else {
            target.push(flag);
        }
    }
}

fn same_flag(left: &FlagDef, right: &FlagDef) -> bool {
    left.long.is_some() && left.long == right.long
        || left.short.is_some() && left.short == right.short
}

fn split_shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut single = false;
    let mut double = false;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !single => escaped = true,
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            ch if ch.is_whitespace() && !single && !double => {
                if !current.is_empty() {
                    words.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn split_path_like(value: &str) -> Vec<&str> {
    if value.contains(';') {
        value.split(';').collect()
    } else {
        value.split(':').collect()
    }
}

fn expand_value(value: &str, env_map: &HashMap<String, String>) -> String {
    let mut out = unquote(value);
    if let Some(home) = env_map.get("HOME") {
        if out == "~" {
            out = home.clone();
        } else if let Some(rest) = out.strip_prefix("~/") {
            out = format!("{}/{}", home, rest);
        } else if let Some(rest) = out.strip_prefix("~\\") {
            out = format!("{}\\{}", home, rest);
        }
    }

    for (key, val) in env_map {
        out = out.replace(&format!("${{{}}}", key), val);
        out = out.replace(&format!("${}", key), val);
    }
    out
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn is_path_var_ref(value: &str) -> bool {
    matches!(value, "$PATH" | "${PATH}" | "$path" | "${path}")
}

fn current_path_entries() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

fn normalise_path_key(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text
    }
}

fn is_safe_env_key(key: &str) -> bool {
    is_env_identifier(key)
        && !matches!(
            key,
            "PATH"
                | "BASH"
                | "BASHOPTS"
                | "BASH_ALIASES"
                | "BASH_CMDS"
                | "BASH_VERSINFO"
                | "EUID"
                | "IFS"
                | "OPTARG"
                | "OPTIND"
                | "PIPESTATUS"
                | "SHELLOPTS"
                | "UID"
        )
        && !key.starts_with("__RUBASH_")
}

fn is_env_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(ch) if ch == '_' || ch.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_identifierish(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(ch) if ch == '_' || ch.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '_' || ch == '-' || ch == '!' || ch.is_ascii_alphanumeric())
}

fn is_safe_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch == '_' || ch == '-' || ch == '.' || ch.is_ascii_alphanumeric())
}

#[allow(dead_code)]
fn _editor_mode_name(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Emacs => "emacs",
        EditorMode::Vi => "vi",
    }
}
