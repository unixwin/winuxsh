//! Safe zsh / Oh My Zsh compatibility scanner.
//!
//! This module reads zsh-style config and plugin assets, but never executes
//! zsh scripts. It produces a report that can be shown to users or later
//! applied through explicit, safe winuxsh hooks.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

use crate::completion::external::{CommandDef, FlagDef, PathLiteral, ValuesSource};
use crate::completion::runtime::RuntimeCompletionCommand;
use crate::config::{EditorMode, ZshCompatLevel, ZshConfig};

pub const ZSH_IMPORT_BLOCK_START: &str = "# >>> winuxsh zsh compat import >>>";
pub const ZSH_IMPORT_BLOCK_END: &str = "# <<< winuxsh zsh compat import <<<";

const NATIVE_GIT_ALIASES: &[(&str, &str)] = &[
    ("g", "git"),
    ("ga", "git add"),
    ("gaa", "git add --all"),
    ("gap", "git apply"),
    ("gapa", "git add --patch"),
    ("gau", "git add --update"),
    ("gb", "git branch"),
    ("gba", "git branch --all"),
    ("gbd", "git branch --delete"),
    ("gbD", "git branch --delete --force"),
    ("gbr", "git branch --remote"),
    ("gcb", "git checkout -b"),
    ("gcl", "git clone --recurse-submodules"),
    ("gco", "git checkout"),
    ("gc", "git commit --verbose"),
    ("gca", "git commit --verbose --all"),
    ("gcam", "git commit --all --message"),
    ("gcmsg", "git commit --message"),
    ("gd", "git diff"),
    ("gdca", "git diff --cached"),
    ("gds", "git diff --staged"),
    ("gf", "git fetch"),
    ("gfo", "git fetch origin"),
    ("gl", "git pull"),
    ("glo", "git log --oneline --decorate"),
    ("glog", "git log --oneline --decorate --graph"),
    ("gp", "git push"),
    ("gpd", "git push --dry-run"),
    ("gpf", "git push --force-with-lease"),
    ("grb", "git rebase"),
    ("grba", "git rebase --abort"),
    ("grbc", "git rebase --continue"),
    ("grbi", "git rebase --interactive"),
    ("grh", "git reset"),
    ("grhh", "git reset --hard"),
    ("grs", "git restore"),
    ("grst", "git restore --staged"),
    ("gst", "git status"),
    ("gss", "git status --short"),
    ("gsb", "git status --short --branch"),
    ("gsta", "git stash push"),
    ("gstaa", "git stash apply"),
    ("gstd", "git stash drop"),
    ("gstl", "git stash list"),
    ("gstp", "git stash pop"),
    ("gsts", "git stash show --patch"),
    ("gsw", "git switch"),
    ("gswc", "git switch --create"),
];

const NATIVE_DOCKER_ALIASES: &[(&str, &str)] = &[
    ("dbl", "docker build"),
    ("dcin", "docker container inspect"),
    ("dcls", "docker container ls"),
    ("dclsa", "docker container ls -a"),
    ("dcprune", "docker container prune"),
    ("dib", "docker image build"),
    ("dii", "docker image inspect"),
    ("dils", "docker image ls"),
    ("dipu", "docker image push"),
    ("dipru", "docker image prune -a"),
    ("dirm", "docker image rm"),
    ("dit", "docker image tag"),
    ("dlo", "docker container logs"),
    ("dnc", "docker network create"),
    ("dncn", "docker network connect"),
    ("dndcn", "docker network disconnect"),
    ("dni", "docker network inspect"),
    ("dnls", "docker network ls"),
    ("dnprune", "docker network prune"),
    ("dnrm", "docker network rm"),
    ("dpo", "docker container port"),
    ("dps", "docker ps"),
    ("dpsa", "docker ps -a"),
    ("dpu", "docker pull"),
    ("dr", "docker container run"),
    ("drit", "docker container run -it"),
    ("drm", "docker container rm"),
    ("drm!", "docker container rm -f"),
    ("dsprune", "docker system prune"),
    ("dst", "docker container start"),
    ("drs", "docker container restart"),
    ("dsta", "docker stop $(docker ps -q)"),
    ("dstp", "docker container stop"),
    ("dsts", "docker stats"),
    ("dtop", "docker top"),
    ("dvi", "docker volume inspect"),
    ("dvls", "docker volume ls"),
    ("dvprune", "docker volume prune"),
    ("dxc", "docker container exec"),
    ("dxcit", "docker container exec -it"),
];

const NATIVE_KUBECTL_ALIASES: &[(&str, &str)] = &[
    ("k", "kubectl"),
    ("kaf", "kubectl apply -f"),
    ("kapk", "kubectl apply -k"),
    ("keti", "kubectl exec -t -i"),
    ("kcuc", "kubectl config use-context"),
    ("kcsc", "kubectl config set-context"),
    ("kcdc", "kubectl config delete-context"),
    ("kccc", "kubectl config current-context"),
    ("kcgc", "kubectl config get-contexts"),
    ("kdel", "kubectl delete"),
    ("kdelf", "kubectl delete -f"),
    ("kdelk", "kubectl delete -k"),
    ("kge", "kubectl get events --sort-by=\".lastTimestamp\""),
    ("kgew", "kubectl get events --sort-by=\".lastTimestamp\" --watch"),
    ("kgp", "kubectl get pods"),
    ("kgpl", "kubectl get pods -l"),
    ("kgpn", "kubectl get pods -n"),
    ("kgpsl", "kubectl get pods --show-labels"),
    ("kgpa", "kubectl get pods --all-namespaces"),
    ("kgpw", "kubectl get pods --watch"),
    ("kgpwide", "kubectl get pods -o wide"),
    ("kep", "kubectl edit pods"),
    ("kdp", "kubectl describe pods"),
    ("kdelp", "kubectl delete pods"),
    ("kgpall", "kubectl get pods --all-namespaces -o wide"),
    ("kgs", "kubectl get svc"),
    ("kgsa", "kubectl get svc --all-namespaces"),
    ("kgsw", "kubectl get svc --watch"),
    ("kgswide", "kubectl get svc -o wide"),
    ("kes", "kubectl edit svc"),
    ("kds", "kubectl describe svc"),
    ("kdels", "kubectl delete svc"),
    ("kgi", "kubectl get ingress"),
    ("kgia", "kubectl get ingress --all-namespaces"),
    ("kei", "kubectl edit ingress"),
    ("kdi", "kubectl describe ingress"),
    ("kdeli", "kubectl delete ingress"),
    ("kgns", "kubectl get namespaces"),
    ("kens", "kubectl edit namespace"),
    ("kdns", "kubectl describe namespace"),
    ("kdelns", "kubectl delete namespace"),
    ("kcn", "kubectl config set-context --current --namespace"),
    ("kgcm", "kubectl get configmaps"),
    ("kgcma", "kubectl get configmaps --all-namespaces"),
    ("kecm", "kubectl edit configmap"),
    ("kdcm", "kubectl describe configmap"),
    ("kdelcm", "kubectl delete configmap"),
    ("kgsec", "kubectl get secret"),
    ("kgseca", "kubectl get secret --all-namespaces"),
    ("kdsec", "kubectl describe secret"),
    ("kdelsec", "kubectl delete secret"),
    ("kgd", "kubectl get deployment"),
    ("kgda", "kubectl get deployment --all-namespaces"),
    ("kgdw", "kubectl get deployment --watch"),
    ("kgdwide", "kubectl get deployment -o wide"),
    ("ked", "kubectl edit deployment"),
    ("kdd", "kubectl describe deployment"),
    ("kdeld", "kubectl delete deployment"),
    ("ksd", "kubectl scale deployment"),
    ("krsd", "kubectl rollout status deployment"),
    ("krrd", "kubectl rollout restart deployment"),
    ("kgrs", "kubectl get replicaset"),
    ("kdrs", "kubectl describe replicaset"),
    ("kers", "kubectl edit replicaset"),
    ("krh", "kubectl rollout history"),
    ("kru", "kubectl rollout undo"),
    ("kgss", "kubectl get statefulset"),
    ("kgssa", "kubectl get statefulset --all-namespaces"),
    ("kgssw", "kubectl get statefulset --watch"),
    ("kgsswide", "kubectl get statefulset -o wide"),
    ("kess", "kubectl edit statefulset"),
    ("kdss", "kubectl describe statefulset"),
    ("kdelss", "kubectl delete statefulset"),
    ("ksss", "kubectl scale statefulset"),
    ("krsss", "kubectl rollout status statefulset"),
    ("krrss", "kubectl rollout restart statefulset"),
    ("kpf", "kubectl port-forward"),
    ("kga", "kubectl get all"),
    ("kgaa", "kubectl get all --all-namespaces"),
    ("kl", "kubectl logs"),
    ("kl1h", "kubectl logs --since 1h"),
    ("kl1m", "kubectl logs --since 1m"),
    ("kl1s", "kubectl logs --since 1s"),
    ("klf", "kubectl logs -f"),
    ("klf1h", "kubectl logs --since 1h -f"),
    ("klf1m", "kubectl logs --since 1m -f"),
    ("klf1s", "kubectl logs --since 1s -f"),
    ("kcp", "kubectl cp"),
    ("kgno", "kubectl get nodes"),
    ("kgnosl", "kubectl get nodes --show-labels"),
    ("keno", "kubectl edit node"),
    ("kdno", "kubectl describe node"),
    ("kdelno", "kubectl delete node"),
    ("kgpvc", "kubectl get pvc"),
    ("kgpvca", "kubectl get pvc --all-namespaces"),
    ("kgpvcw", "kubectl get pvc --watch"),
    ("kepvc", "kubectl edit pvc"),
    ("kdpvc", "kubectl describe pvc"),
    ("kdelpvc", "kubectl delete pvc"),
    ("kdsa", "kubectl describe sa"),
    ("kdelsa", "kubectl delete sa"),
    ("kgds", "kubectl get daemonset"),
    ("kgdsa", "kubectl get daemonset --all-namespaces"),
    ("kgdsw", "kubectl get daemonset --watch"),
    ("keds", "kubectl edit daemonset"),
    ("kdds", "kubectl describe daemonset"),
    ("kdelds", "kubectl delete daemonset"),
    ("kgcj", "kubectl get cronjob"),
    ("kecj", "kubectl edit cronjob"),
    ("kdcj", "kubectl describe cronjob"),
    ("kdelcj", "kubectl delete cronjob"),
    ("kgj", "kubectl get job"),
    ("kej", "kubectl edit job"),
    ("kdj", "kubectl describe job"),
    ("kdelj", "kubectl delete job"),
];

const NATIVE_NPM_ALIASES: &[(&str, &str)] = &[
    ("npmg", "npm i -g"),
    ("npmS", "npm i -S"),
    ("npmD", "npm i -D"),
    ("npmF", "npm i -f"),
    ("npmO", "npm outdated"),
    ("npmU", "npm update"),
    ("npmV", "npm -v"),
    ("npmL", "npm list"),
    ("npmL0", "npm ls --depth=0"),
    ("npmst", "npm start"),
    ("npmt", "npm test"),
    ("npmR", "npm run"),
    ("npmP", "npm publish"),
    ("npmI", "npm init"),
    ("npmi", "npm info"),
    ("npmSe", "npm search"),
    ("npmrd", "npm run dev"),
    ("npmrb", "npm run build"),
];

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
    pub prompt: Option<ImportedPrompt>,
    pub right_prompt: Option<ImportedPrompt>,
    pub git_prompt: ImportedGitPrompt,
    pub edit_mode: Option<String>,
    pub zstyles: Vec<ImportedZstyle>,
    pub highlight_styles: Vec<ImportedHighlightStyle>,
    pub completion_assets: Vec<CompletionAsset>,
    pub dynamic_completion_sources: Vec<DynamicCompletionSource>,
    pub native_hooks: Vec<NativeHookSuggestion>,
    pub native_widgets: Vec<NativeWidgetSuggestion>,
    pub zsh_functions: Vec<ZshFunctionSuggestion>,
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
        out.push(format!(
            "dynamic completion sources: {}",
            self.dynamic_completion_sources.len()
        ));
        out.push(format!("native hook suggestions: {}", self.native_hooks.len()));
        out.push(format!(
            "native widget suggestions: {}",
            self.native_widgets.len()
        ));
        out.push(format!(
            "zsh function suggestions: {}",
            self.zsh_functions.len()
        ));
        out.push(format!("zstyles: {}", self.zstyles.len()));
        out.push(format!("highlight styles: {}", self.highlight_styles.len()));
        out.push(format!(
            "theme: {}",
            self.theme.as_deref().unwrap_or("(none)")
        ));
        out.push(format!(
            "prompt: {}",
            self.prompt
                .as_ref()
                .and_then(|prompt| prompt.translated_format.as_deref())
                .unwrap_or("(none)")
        ));
        out.push(format!(
            "right prompt: {}",
            self.right_prompt
                .as_ref()
                .and_then(|prompt| prompt.translated_format.as_deref())
                .unwrap_or("(none)")
        ));
        out.push(format!(
            "git prompt: {}",
            git_prompt_format_from_report(self).unwrap_or_else(|| "(none)".to_string())
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

        if !self.dynamic_completion_sources.is_empty() {
            out.push("dynamic completion detail:".to_string());
            for source in &self.dynamic_completion_sources {
                out.push(format!(
                    "  - {} {} kind={:?} target={} origin={}",
                    source.command,
                    source.args.join(" "),
                    source.kind,
                    source.target_shell,
                    source.origin
                ));
            }
        }

        if !self.native_hooks.is_empty() {
            out.push("native hook suggestions:".to_string());
            for hook in &self.native_hooks {
                out.push(format!(
                    "  - {} {} origin={}",
                    hook.hook, hook.function, hook.origin
                ));
            }
        }

        if !self.native_widgets.is_empty() {
            out.push("native widget suggestions:".to_string());
            for widget in &self.native_widgets {
                let function = widget
                    .function
                    .as_ref()
                    .map(|function| format!(" function={}", function))
                    .unwrap_or_default();
                let binding = match (&widget.keymap, &widget.key) {
                    (Some(keymap), Some(key)) => format!(" keymap={} key={}", keymap, key),
                    (None, Some(key)) => format!(" key={}", key),
                    _ => String::new(),
                };
                out.push(format!(
                    "  - {}{}{} origin={}",
                    widget.widget, function, binding, widget.origin
                ));
            }
        }

        if !self.zsh_functions.is_empty() {
            out.push("zsh function suggestions:".to_string());
            for function in &self.zsh_functions {
                out.push(format!(
                    "  - {} kind={} autoloaded={} origin={}",
                    function.function,
                    zsh_function_kind_name(function.kind),
                    function.autoloaded,
                    function.origin
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
pub struct ImportedHighlightStyle {
    pub key: String,
    pub value: String,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedPrompt {
    pub value: String,
    pub translated_format: Option<String>,
    pub unsupported_segments: Vec<String>,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportedGitPrompt {
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub dirty: Option<String>,
    pub clean: Option<String>,
    pub variables: Vec<ImportedGitPromptVar>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedGitPromptVar {
    pub key: String,
    pub value: String,
    pub translated_value: Option<String>,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZshPromptTranslation {
    pub format: Option<String>,
    pub unsupported_segments: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionAsset {
    pub source_file: PathBuf,
    pub commands: Vec<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DynamicCompletionSource {
    pub kind: DynamicCompletionKind,
    pub command: String,
    pub args: Vec<String>,
    pub target_shell: String,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NativeHookSuggestion {
    pub hook: String,
    pub function: String,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NativeWidgetSuggestion {
    pub widget: String,
    pub function: Option<String>,
    pub key: Option<String>,
    pub keymap: Option<String>,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ZshFunctionSuggestion {
    pub function: String,
    pub kind: ZshFunctionKind,
    pub autoloaded: bool,
    pub source_file: Option<PathBuf>,
    pub line: Option<usize>,
    pub origin: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ZshFunctionKind {
    CompletionHelper,
    LifecycleHelper,
    WidgetHelper,
    PromptHelper,
    GenericHelper,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DynamicCompletionKind {
    ScriptGenerator,
    RuntimeProvider,
}

#[derive(Debug, Clone)]
pub struct DynamicCompletionRunOptions {
    pub allowed_commands: Vec<String>,
    pub timeout: Duration,
    pub cache_dir: Option<PathBuf>,
    pub cache_ttl: Option<Duration>,
}

impl Default for DynamicCompletionRunOptions {
    fn default() -> Self {
        Self {
            allowed_commands: Vec::new(),
            timeout: Duration::from_millis(1500),
            cache_dir: None,
            cache_ttl: None,
        }
    }
}

impl DynamicCompletionRunOptions {
    pub fn from_zsh_config(config: &ZshConfig) -> Option<Self> {
        let dynamic = &config.dynamic_completions;
        if !dynamic.enabled || dynamic.commands.is_empty() {
            return None;
        }

        Some(Self {
            allowed_commands: dynamic.commands.clone(),
            timeout: Duration::from_millis(dynamic.timeout_millis.max(1)),
            cache_dir: Some(
                dynamic
                    .cache_dir
                    .clone()
                    .unwrap_or_else(default_dynamic_completion_cache_dir),
            ),
            cache_ttl: dynamic.cache_ttl_secs.map(Duration::from_secs),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DynamicCompletionDiskCache {
    command: String,
    args: Vec<String>,
    target_shell: String,
    written_secs: u64,
    ttl_secs: u64,
    output: String,
}

impl DynamicCompletionDiskCache {
    fn is_for_source(&self, source: &DynamicCompletionSource) -> bool {
        self.command == source.command
            && self.args == source.args
            && self.target_shell == source.target_shell
    }

    fn is_fresh(&self) -> bool {
        if self.ttl_secs == 0 {
            return true;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.written_secs) <= self.ttl_secs
    }
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
    Theme,
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

    refresh_prompt_translations(&mut report);

    report
}

pub fn git_prompt_format_from_report(report: &ZshImportReport) -> Option<String> {
    if report.git_prompt.variables.is_empty() {
        return None;
    }
    let mut format = String::new();
    if let Some(prefix) = &report.git_prompt.prefix {
        format.push_str(prefix);
    }
    format.push_str("{git_branch}");
    if let Some(suffix) = &report.git_prompt.suffix {
        format.push_str(suffix);
    }
    Some(clean_prompt_template(&format))
}

pub fn import_plan_toml(options: &ZshImportOptions, report: &ZshImportReport) -> String {
    let mut out = Vec::new();
    out.push("# Generated by winuxsh --zsh-compat-import-plan".to_string());
    out.push("# Review before appending to ~/.winshrc.toml.".to_string());
    out.push("# Unsupported features remain visible in --zsh-compat-report.".to_string());
    out.push(String::new());

    out.push("[zsh]".to_string());
    out.push("enabled = true".to_string());
    out.push("auto_apply = true".to_string());
    out.push(format!(
        "zdotdir = {}",
        toml_quote(&options.zdotdir.to_string_lossy())
    ));
    out.push(format!("import_zshrc = {}", options.import_zshrc));
    out.push(format!("import_oh_my_zsh = {}", options.import_oh_my_zsh));
    out.push(format!(
        "compat_level = {}",
        toml_quote(compat_level_name(options.compat_level))
    ));
    let plugins = plugin_names_for_import_plan(options, report);
    if !plugins.is_empty() {
        out.push(format!("plugins = {}", toml_array(&plugins)));
    }

    if let Some(edit_mode) = &report.edit_mode {
        out.push(String::new());
        out.push("[editor]".to_string());
        out.push(format!("edit_mode = {}", toml_quote(edit_mode)));
    }

    let prompt = report
        .prompt
        .as_ref()
        .and_then(|prompt| prompt.translated_format.as_deref());
    let right_prompt = report
        .right_prompt
        .as_ref()
        .and_then(|prompt| prompt.translated_format.as_deref());
    if prompt.is_some() || right_prompt.is_some() {
        out.push(String::new());
        out.push("[shell]".to_string());
        if let Some(prompt) = prompt {
            out.push(format!("prompt_format = {}", toml_quote(prompt)));
        }
        if let Some(right_prompt) = right_prompt {
            out.push(format!(
                "right_prompt_format = {}",
                toml_quote(right_prompt)
            ));
        }
    }

    let aliases = aliases_for_import_plan(report);
    if !aliases.is_empty() {
        out.push(String::new());
        out.push("[aliases]".to_string());
        for (name, value) in aliases {
            out.push(format!("{} = {}", toml_quote(&name), toml_quote(&value)));
        }
    }

    if !report.completion_assets.is_empty() {
        out.push(String::new());
        out.push(format!(
            "# zsh completion assets detected: {}",
            report.completion_assets.len()
        ));
        out.push("# They are translated at startup by [zsh].auto_apply.".to_string());
    }

    let script_dynamic_count = dynamic_completion_script_generator_count(report);
    if script_dynamic_count > 0 {
        out.push(String::new());
        out.push(format!(
            "# dynamic zsh completion generators detected: {}",
            script_dynamic_count
        ));
        out.push("# They remain disabled until you explicitly set enabled = true.".to_string());
        out.push("[zsh.dynamic_completions]".to_string());
        out.push("enabled = false".to_string());
        out.push(format!(
            "commands = {}",
            toml_array(&dynamic_completion_commands_for_import_plan(report))
        ));
        out.push("timeout_millis = 1500".to_string());
        out.push("cache_ttl_secs = 86400".to_string());
    }

    let runtime_dynamic_count = dynamic_completion_runtime_provider_count(report);
    if runtime_dynamic_count > 0 {
        out.push(String::new());
        out.push(format!(
            "# runtime zsh completion providers detected: {}",
            runtime_dynamic_count
        ));
        out.push(
            "# They depend on the current input buffer and need native winuxsh providers."
                .to_string(),
        );
        out.push("# They remain disabled until you explicitly set enabled = true.".to_string());
        out.push("[zsh.runtime_completions]".to_string());
        out.push("enabled = false".to_string());
        out.push(format!(
            "commands = {}",
            toml_array(&runtime_completion_commands_for_import_plan(report))
        ));
        out.push("timeout_millis = 1000".to_string());
    }

    let native_plugin_presets = native_plugin_presets_for_import_plan(report);
    if !native_plugin_presets.is_empty() {
        out.push(String::new());
        out.push("# native dynamic zsh plugin presets detected".to_string());
        out.push("# They remain disabled until you explicitly set enabled = true.".to_string());
        out.push("# winuxsh implements these through native hooks/providers, not zsh sourcing.".to_string());
        out.push("[zsh.native_plugins]".to_string());
        out.push("enabled = false".to_string());
        out.push(format!("presets = {}", toml_array(&native_plugin_presets)));
    }

    if !report.native_hooks.is_empty() {
        out.push(String::new());
        out.push(format!(
            "# zsh lifecycle hooks detected: {}",
            report.native_hooks.len()
        ));
        out.push("# Review and translate these zsh functions before enabling native hooks.".to_string());
        out.push("# winuxsh never sources zsh hook function bodies directly.".to_string());
        out.push("# [hooks]".to_string());
        for hook in ["precmd", "preexec", "chpwd"] {
            let scripts = native_hook_todos_for_import_plan(report, hook);
            if !scripts.is_empty() {
                out.push(format!("# {} = {}", hook, toml_array(&scripts)));
            }
        }
    }

    if !report.zsh_functions.is_empty() {
        out.push(String::new());
        out.push(format!(
            "# zsh autoload/function helpers detected: {}",
            report.zsh_functions.len()
        ));
        out.push("# Review before translating; winuxsh never sources zsh function bodies directly.".to_string());
        out.push("# Function suggestions are future native helpers, providers, or presets.".to_string());
        for todo in zsh_function_todos_for_import_plan(report) {
            out.push(format!("# {}", todo));
        }
    }

    let native_widget_presets = native_widget_presets_for_import_plan(report);
    if !report.native_widgets.is_empty() || !native_widget_presets.is_empty() {
        out.push(String::new());
        if !report.native_widgets.is_empty() {
            out.push(format!(
                "# zsh ZLE widgets/keybindings detected: {}",
                report.native_widgets.len()
            ));
        } else {
            out.push("# zsh native widget presets detected from plugin names".to_string());
        }
        out.push("# Review and translate these into native reedline widgets/keybindings.".to_string());
        out.push("# winuxsh never sources ZLE widget function bodies directly.".to_string());
        if !native_widget_presets.is_empty() {
            out.push("[zsh.native_widgets]".to_string());
            out.push("enabled = false".to_string());
            out.push(format!("presets = {}", toml_array(&native_widget_presets)));
            out.push("import_bindkeys = true".to_string());
        }
        for todo in native_widget_todos_for_import_plan(report) {
            out.push(format!("# {}", todo));
        }
    }

    out.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZshImportApplySummary {
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub replaced_existing_block: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZshImportBlockState {
    Missing,
    Present,
    Malformed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZshImportApplyReadiness {
    AddNewBlock,
    ReplaceExistingBlock,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZshImportConfigStatus {
    pub config_path: PathBuf,
    pub config_exists: bool,
    pub block_state: ZshImportBlockState,
    pub toml_valid: bool,
    pub toml_error: Option<String>,
    pub apply_readiness: ZshImportApplyReadiness,
    pub apply_error: Option<String>,
    pub backup_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZshImportRollbackPlan {
    pub config_path: PathBuf,
    pub backup_paths: Vec<PathBuf>,
    pub latest_backup_path: Option<PathBuf>,
    pub restore_command: Option<String>,
}

pub fn apply_import_plan_to_config(
    config_path: &Path,
    plan: &str,
) -> anyhow::Result<ZshImportApplySummary> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
        .to_string();
    apply_import_plan_to_config_with_backup_suffix(config_path, plan, &suffix)
}

pub fn inspect_import_config_status(
    config_path: &Path,
    plan: &str,
) -> anyhow::Result<ZshImportConfigStatus> {
    let (config_exists, original) = match std::fs::read_to_string(config_path) {
        Ok(content) => (true, content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (false, String::new()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read {}", config_path.display()))
        }
    };

    let block_state = managed_import_block_state(&original);
    let (toml_valid, toml_error) = match toml::from_str::<toml::Value>(&original) {
        Ok(_) => (true, None),
        Err(err) => (false, Some(err.to_string())),
    };

    let (apply_readiness, apply_error) = match preview_import_plan_update(&original, plan) {
        Ok(preview) if preview.replaced_existing_block => {
            (ZshImportApplyReadiness::ReplaceExistingBlock, None)
        }
        Ok(_) => (ZshImportApplyReadiness::AddNewBlock, None),
        Err(err) => (
            ZshImportApplyReadiness::Blocked,
            Some(err.to_string()),
        ),
    };

    Ok(ZshImportConfigStatus {
        config_path: config_path.to_path_buf(),
        config_exists,
        block_state,
        toml_valid,
        toml_error,
        apply_readiness,
        apply_error,
        backup_paths: backup_paths_for(config_path)?,
    })
}

pub fn inspect_import_rollback_plan(config_path: &Path) -> anyhow::Result<ZshImportRollbackPlan> {
    let backup_paths = backup_paths_for(config_path)?;
    let latest_backup_path = backup_paths.last().cloned();
    let restore_command = latest_backup_path.as_ref().map(|backup_path| {
        format!(
            "Copy-Item -LiteralPath {} -Destination {} -Force",
            powershell_single_quote_path(backup_path),
            powershell_single_quote_path(config_path)
        )
    });

    Ok(ZshImportRollbackPlan {
        config_path: config_path.to_path_buf(),
        backup_paths,
        latest_backup_path,
        restore_command,
    })
}

pub fn zsh_compat_doctor_text(
    report: &ZshImportReport,
    status: &ZshImportConfigStatus,
    rollback: &ZshImportRollbackPlan,
) -> String {
    let info_count = diagnostic_count(report, DiagnosticSeverity::Info);
    let warn_count = diagnostic_count(report, DiagnosticSeverity::Warn);
    let unsupported_count = diagnostic_count(report, DiagnosticSeverity::Unsupported);

    let safe_plugin_count = plugin_tier_count(report, PluginImportTier::Tier1Safe);
    let partial_plugin_count = plugin_tier_count(report, PluginImportTier::Tier2Partial);
    let native_plugin_count = plugin_tier_count(report, PluginImportTier::Tier3Native);
    let unsupported_plugin_count = plugin_tier_count(report, PluginImportTier::Tier4Unsupported);
    let missing_plugin_count = plugin_tier_count(report, PluginImportTier::Missing);

    let prompt_ready = report
        .prompt
        .as_ref()
        .and_then(|prompt| prompt.translated_format.as_ref())
        .is_some()
        || report
            .right_prompt
            .as_ref()
            .and_then(|prompt| prompt.translated_format.as_ref())
            .is_some();
    let git_prompt_ready = git_prompt_format_from_report(report).is_some();

    let mut out = Vec::new();
    out.push("Zsh compatibility doctor".to_string());
    out.push(format!("Config: {}", status.config_path.display()));
    out.push(format!(
        "Discovered: source files={}, oh-my-zsh={}",
        report.source_files.len(),
        yes_no(report.oh_my_zsh_detected)
    ));
    out.push(format!(
        "Imports: aliases={}, env={}, PATH entries={}, completions={}, plugins={}",
        report.aliases.len(),
        report.env.len(),
        report.path_entries.len(),
        report.completion_assets.len(),
        report.plugins.len()
    ));
    out.push(format!(
        "Dynamic completion generators: {}",
        report.dynamic_completion_sources.len()
    ));
    out.push(format!(
        "Plugin tiers: safe={}, partial={}, native={}, unsupported={}, missing={}",
        safe_plugin_count,
        partial_plugin_count,
        native_plugin_count,
        unsupported_plugin_count,
        missing_plugin_count
    ));
    out.push(format!(
        "Native UX: edit_mode={}, prompt={}, git_prompt={}, highlight_styles={}",
        report.edit_mode.as_deref().unwrap_or("(none)"),
        yes_no(prompt_ready),
        yes_no(git_prompt_ready),
        report.highlight_styles.len()
    ));
    out.push(format!(
        "Diagnostics: info={}, warnings={}, unsupported={}",
        info_count, warn_count, unsupported_count
    ));
    out.push(format!(
        "Import config: exists={}, managed_block={}, toml={}",
        yes_no(status.config_exists),
        block_state_label(status.block_state),
        if status.toml_valid { "valid" } else { "invalid" }
    ));
    if let Some(error) = &status.toml_error {
        out.push(format!("TOML detail: {}", error));
    }
    out.push(format!(
        "Next apply: {}",
        apply_readiness_label(status.apply_readiness)
    ));
    if let Some(error) = &status.apply_error {
        out.push(format!("Apply detail: {}", error));
    }
    out.push(format!(
        "Rollback: backups={}, latest={}",
        rollback.backup_paths.len(),
        rollback
            .latest_backup_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string())
    ));
    if let Some(command) = &rollback.restore_command {
        out.push(format!("Restore latest backup: {}", command));
    }

    out.push("Next commands:".to_string());
    out.push("  - Detailed report: winuxsh --zsh-compat-report".to_string());
    out.push("  - Review import patch: winuxsh --zsh-compat-import-plan".to_string());
    match status.apply_readiness {
        ZshImportApplyReadiness::AddNewBlock | ZshImportApplyReadiness::ReplaceExistingBlock => {
            out.push("  - Apply safe import: winuxsh --zsh-compat-import-apply".to_string());
        }
        ZshImportApplyReadiness::Blocked => {
            out.push(
                "  - Resolve blocker or merge manually before running import-apply".to_string(),
            );
        }
    }
    out.push("  - Rollback plan: winuxsh --zsh-compat-import-rollback-plan".to_string());

    out.join("\n")
}

#[doc(hidden)]
pub fn apply_import_plan_to_config_with_backup_suffix(
    config_path: &Path,
    plan: &str,
    backup_suffix: &str,
) -> anyhow::Result<ZshImportApplySummary> {
    let original = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read {}", config_path.display()))
        }
    };

    let preview = preview_import_plan_update(&original, plan)?;

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let backup_path = if config_path.exists() {
        let backup_path = unique_backup_path_for(config_path, backup_suffix);
        std::fs::copy(config_path, &backup_path).with_context(|| {
            format!(
                "failed to create config backup {}",
                backup_path.display()
            )
        })?;
        Some(backup_path)
    } else {
        None
    };

    std::fs::write(config_path, preview.next_config)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    Ok(ZshImportApplySummary {
        config_path: config_path.to_path_buf(),
        backup_path,
        replaced_existing_block: preview.replaced_existing_block,
    })
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

pub fn dynamic_completion_defs_from_report_with_runner<F>(
    report: &ZshImportReport,
    mut runner: F,
) -> Vec<CommandDef>
where
    F: FnMut(&DynamicCompletionSource) -> Result<String, String>,
{
    let mut definitions: HashMap<String, CommandDef> = HashMap::new();

    for source in &report.dynamic_completion_sources {
        if source.kind != DynamicCompletionKind::ScriptGenerator
            || source.target_shell != "zsh"
            || !is_safe_name(&source.command)
        {
            continue;
        }
        let Ok(output) = runner(source) else {
            continue;
        };
        let flags = parse_zsh_argument_flags(&output);
        if flags.is_empty() {
            continue;
        }
        let def = definitions
            .entry(source.command.clone())
            .or_insert_with(|| CommandDef {
                command: source.command.clone(),
                description: Some(format!(
                    "Generated from dynamic zsh completion source: {} {}",
                    source.command,
                    source.args.join(" ")
                )),
                flags: Vec::new(),
                subcommands: Vec::new(),
            });
        merge_flags(&mut def.flags, flags);
    }

    let mut values: Vec<CommandDef> = definitions.into_values().collect();
    values.sort_by(|left, right| left.command.cmp(&right.command));
    values
}

pub fn dynamic_completion_defs_from_report_with_options(
    report: &ZshImportReport,
    options: &DynamicCompletionRunOptions,
) -> Vec<CommandDef> {
    dynamic_completion_defs_from_report_with_runner(report, |source| {
        cached_or_run_dynamic_completion_source(source, options)
    })
}

pub fn runtime_completion_commands_from_report(
    report: &ZshImportReport,
    allowed_commands: &[String],
) -> Vec<RuntimeCompletionCommand> {
    let allowed: HashSet<&str> = allowed_commands.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let mut commands = Vec::new();

    for source in &report.dynamic_completion_sources {
        if source.kind != DynamicCompletionKind::RuntimeProvider
            || source.target_shell != "words"
            || !is_safe_name(&source.command)
            || !allowed.contains(source.command.as_str())
            || !seen.insert(source.command.clone())
        {
            continue;
        }

        commands.push(RuntimeCompletionCommand {
            command: source.command.clone(),
            args: source.args.clone(),
            origin: source.origin.clone(),
        });
    }

    commands.sort_by(|left, right| left.command.cmp(&right.command));
    commands
}

fn cached_or_run_dynamic_completion_source(
    source: &DynamicCompletionSource,
    options: &DynamicCompletionRunOptions,
) -> Result<String, String> {
    let cached = read_dynamic_completion_cache(source, options);
    if let Some(cache) = cached.as_ref().filter(|cache| cache.is_fresh()) {
        return Ok(cache.output.clone());
    }

    match run_dynamic_completion_source(source, options) {
        Ok(output) => {
            write_dynamic_completion_cache(source, options, &output);
            Ok(output)
        }
        Err(err) => {
            if let Some(cache) = cached {
                log::warn!(
                    "dynamic zsh completion generator failed; using stale cache for {}: {}",
                    source.command,
                    err
                );
                Ok(cache.output)
            } else {
                Err(err)
            }
        }
    }
}

fn run_dynamic_completion_source(
    source: &DynamicCompletionSource,
    options: &DynamicCompletionRunOptions,
) -> Result<String, String> {
    if source.kind != DynamicCompletionKind::ScriptGenerator {
        return Err(format!(
            "unsupported dynamic completion source kind: {:?}",
            source.kind
        ));
    }
    if source.target_shell != "zsh" {
        return Err(format!(
            "unsupported dynamic completion target shell: {}",
            source.target_shell
        ));
    }
    if !is_safe_name(&source.command) || !is_dynamic_completion_command_allowed(source, options) {
        return Err(format!(
            "dynamic completion command is not allowed: {}",
            source.command
        ));
    }
    if source.args.iter().any(|arg| !is_safe_dynamic_completion_arg(arg)) {
        return Err(format!(
            "dynamic completion command has unsafe args: {}",
            source.args.join(" ")
        ));
    }

    let stdout_path = dynamic_completion_temp_path(&source.command, "stdout");
    let stderr_path = dynamic_completion_temp_path(&source.command, "stderr");
    let stdout = std::fs::File::create(&stdout_path)
        .map_err(|err| format!("failed to create stdout capture: {}", err))?;
    let stderr = std::fs::File::create(&stderr_path)
        .map_err(|err| format!("failed to create stderr capture: {}", err))?;

    let mut child = match Command::new(&source.command)
        .args(&source.args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let _ = std::fs::remove_file(&stdout_path);
            let _ = std::fs::remove_file(&stderr_path);
            return Err(format!("failed to run dynamic completion command: {}", err));
        }
    };

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= options.timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&stdout_path);
                    let _ = std::fs::remove_file(&stderr_path);
                    return Err(format!(
                        "dynamic completion command timed out after {:?}",
                        options.timeout
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&stdout_path);
                let _ = std::fs::remove_file(&stderr_path);
                return Err(format!(
                    "failed while waiting for dynamic completion command: {}",
                    err
                ));
            }
        }
    };

    let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
    let _ = std::fs::remove_file(&stdout_path);
    let _ = std::fs::remove_file(&stderr_path);

    if !status.success() {
        return Err(format!(
            "dynamic completion command exited with {}: {}",
            status,
            stderr.trim()
        ));
    }

    Ok(stdout)
}

fn read_dynamic_completion_cache(
    source: &DynamicCompletionSource,
    options: &DynamicCompletionRunOptions,
) -> Option<DynamicCompletionDiskCache> {
    let path = dynamic_completion_cache_path(source, options)?;
    let content = std::fs::read_to_string(path).ok()?;
    let cache: DynamicCompletionDiskCache = toml::from_str(&content).ok()?;
    cache.is_for_source(source).then_some(cache)
}

fn write_dynamic_completion_cache(
    source: &DynamicCompletionSource,
    options: &DynamicCompletionRunOptions,
    output: &str,
) {
    let Some(path) = dynamic_completion_cache_path(source, options) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(err) = std::fs::create_dir_all(parent) {
        log::warn!(
            "failed to create dynamic zsh completion cache dir {}: {}",
            parent.display(),
            err
        );
        return;
    }

    let cache = DynamicCompletionDiskCache {
        command: source.command.clone(),
        args: source.args.clone(),
        target_shell: source.target_shell.clone(),
        written_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0),
        ttl_secs: options.cache_ttl.map(|ttl| ttl.as_secs()).unwrap_or(0),
        output: output.to_string(),
    };

    match toml::to_string_pretty(&cache) {
        Ok(content) => {
            if let Err(err) = std::fs::write(&path, content) {
                log::warn!(
                    "failed to write dynamic zsh completion cache {}: {}",
                    path.display(),
                    err
                );
            }
        }
        Err(err) => {
            log::warn!("failed to serialize dynamic zsh completion cache: {}", err);
        }
    }
}

fn dynamic_completion_cache_path(
    source: &DynamicCompletionSource,
    options: &DynamicCompletionRunOptions,
) -> Option<PathBuf> {
    let cache_dir = options.cache_dir.as_ref()?;
    Some(cache_dir.join(format!(
        "{}-{}.toml",
        sanitize_cache_component(&source.command),
        dynamic_completion_cache_hash(source)
    )))
}

fn dynamic_completion_cache_hash(source: &DynamicCompletionSource) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.command.hash(&mut hasher);
    source.args.hash(&mut hasher);
    source.target_shell.hash(&mut hasher);
    hasher.finish()
}

fn sanitize_cache_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn is_dynamic_completion_command_allowed(
    source: &DynamicCompletionSource,
    options: &DynamicCompletionRunOptions,
) -> bool {
    options
        .allowed_commands
        .iter()
        .any(|command| command == &source.command)
}

fn is_safe_dynamic_completion_arg(arg: &str) -> bool {
    !arg.is_empty() && !arg.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r'))
}

fn dynamic_completion_temp_path(command: &str, stream: &str) -> PathBuf {
    let safe_command = sanitize_cache_component(command);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "winuxsh-zsh-completion-{}-{}-{}.{}",
        safe_command,
        std::process::id(),
        nanos,
        stream
    ))
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
        for hook in parse_native_hook_suggestions(line, source_file, line_no, mode) {
            push_native_hook_suggestion(report, hook);
        }
        for widget in parse_native_widget_suggestions(line, source_file, line_no, mode) {
            push_native_widget_suggestion(report, widget);
        }
        for function in parse_zsh_function_suggestions(line, source_file, line_no, mode) {
            push_zsh_function_suggestion(report, function);
        }
        if let Some((command, args, target_shell, kind)) = parse_dynamic_completion_source(line) {
            push_dynamic_completion_source(
                report,
                DynamicCompletionSource {
                    kind,
                    command: command.clone(),
                    args: args.clone(),
                    target_shell,
                    source_file: source_file.map(Path::to_path_buf),
                    line: Some(line_no),
                    origin: scan_mode_origin(mode).to_string(),
                },
            );
            report.diagnostics.push(ZshCompatDiagnostic {
                severity: DiagnosticSeverity::Unsupported,
                feature: dynamic_completion_feature(kind).to_string(),
                message: dynamic_completion_message(kind, &command, &args),
                source_file: source_file.map(Path::to_path_buf),
                line: Some(line_no),
            });
        }

        if let Some((name, value)) = parse_alias(line, source_file, line_no, report) {
            report.aliases.push(ImportedAlias {
                name,
                value,
                source_file: source_file.map(Path::to_path_buf),
                line: Some(line_no),
                origin: scan_mode_origin(mode).to_string(),
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

        if let Some((key, value)) = parse_highlight_style_assignment(line) {
            report.highlight_styles.push(ImportedHighlightStyle {
                key,
                value,
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

        if let Some(values) = parse_named_array(line, "plugins") {
            if mode != ScanMode::Profile {
                continue;
            }
            add_plugins(report, values);
            continue;
        }

        if let Some(values) = parse_named_array(line, "path") {
            if mode != ScanMode::Profile {
                continue;
            }
            for value in values {
                add_path_entry(report, env_map, &value, true);
            }
            continue;
        }

        if let Some(values) = parse_named_array(line, "fpath") {
            if mode != ScanMode::Profile {
                continue;
            }
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
            if is_prompt_assignment(&key) && mode != ScanMode::Plugin {
                record_prompt_assignment(
                    key,
                    value,
                    source_file,
                    line_no,
                    report,
                    scan_mode_origin(mode),
                );
                continue;
            }
            if is_git_prompt_assignment(&key) && mode != ScanMode::Plugin {
                record_git_prompt_assignment(
                    key,
                    value,
                    source_file,
                    line_no,
                    report,
                    scan_mode_origin(mode),
                );
                continue;
            }
            if mode != ScanMode::Profile {
                continue;
            }
            record_assignment(key, value, source_file, line_no, report, env_map);
        }
    }
}

fn default_dynamic_completion_cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".winuxsh")
        .join("cache")
        .join("zsh-completions")
}

fn scan_mode_origin(mode: ScanMode) -> &'static str {
    match mode {
        ScanMode::Profile => "profile",
        ScanMode::Plugin => "plugin",
        ScanMode::Theme => "theme",
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

    scan_oh_my_zsh_theme(&zsh_dir, &zsh_custom, report, env_map);

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
            let alias_count = apply_native_plugin_pack(report, &plugin_name);
            let has_dynamic_completion =
                apply_native_dynamic_completion_preset(report, &plugin_name);
            let requires_native_ux = native_plugin_requires_native_ux(&plugin_name);
            if alias_count > 0 || has_dynamic_completion || requires_native_ux {
                report.plugins.push(native_plugin_preset(
                    plugin_name,
                    alias_count,
                    has_dynamic_completion,
                    requires_native_ux,
                ));
            } else {
                report.plugins.push(unresolved_plugin(plugin_name, 1));
            }
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

fn scan_oh_my_zsh_theme(
    zsh_dir: &Path,
    zsh_custom: &Path,
    report: &mut ZshImportReport,
    env_map: &mut HashMap<String, String>,
) {
    let Some(theme_name) = report.theme.clone() else {
        return;
    };
    if theme_name == "random" || theme_name.starts_with('$') || !is_safe_name(&theme_name) {
        report.diagnostics.push(ZshCompatDiagnostic {
            severity: DiagnosticSeverity::Unsupported,
            feature: "theme".to_string(),
            message: format!("dynamic or unsafe zsh theme skipped: {}", theme_name),
            source_file: None,
            line: None,
        });
        return;
    }

    let theme_file_name = format!("{}.zsh-theme", theme_name);
    let Some(theme_file) = [
        zsh_custom.join("themes").join(&theme_file_name),
        zsh_dir.join("themes").join(&theme_file_name),
    ]
    .into_iter()
    .find(|path| path.is_file())
    else {
        report.diagnostics.push(ZshCompatDiagnostic {
            severity: DiagnosticSeverity::Info,
            feature: "theme".to_string(),
            message: format!("zsh theme file not found: {}", theme_name),
            source_file: None,
            line: None,
        });
        return;
    };

    scan_profile_file(&theme_file, report, env_map, ScanMode::Theme);
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

fn native_plugin_preset(
    name: String,
    alias_count: usize,
    has_dynamic_completion: bool,
    requires_native_ux: bool,
) -> ImportedPlugin {
    let mut capabilities = Vec::new();
    let has_native_widget_preset = native_plugin_widget_preset(&name).is_some();
    let has_native_plugin_preset = native_dynamic_plugin_preset(&name).is_some();
    if alias_count > 0 {
        capabilities.push("native_aliases".to_string());
        capabilities.push("aliases".to_string());
    }
    if has_dynamic_completion {
        capabilities.push("dynamic_completions_required".to_string());
    }
    if requires_native_ux {
        capabilities.push("native_ux_required".to_string());
    }
    if has_native_widget_preset {
        capabilities.push("native_widgets_required".to_string());
    }
    if has_native_plugin_preset {
        capabilities.push("native_plugins_required".to_string());
        capabilities.push("native_lifecycle_hooks_required".to_string());
    }

    ImportedPlugin {
        name,
        source_dir: None,
        plugin_script: None,
        completion_files: Vec::new(),
        alias_count,
        diagnostics_count: 0,
        tier: if requires_native_ux {
            PluginImportTier::Tier3Native
        } else if has_dynamic_completion {
            PluginImportTier::Tier2Partial
        } else {
            PluginImportTier::Tier1Safe
        },
        import_kind: if requires_native_ux {
            PluginImportKind::NativeUx
        } else if has_dynamic_completion {
            PluginImportKind::Partial
        } else {
            PluginImportKind::AliasOnly
        },
        capabilities,
        unsupported_features: if requires_native_ux {
            vec!["native-ux-shim".to_string()]
        } else {
            Vec::new()
        },
    }
}

fn apply_native_plugin_pack(report: &mut ZshImportReport, plugin_name: &str) -> usize {
    let Some(aliases) = native_plugin_aliases(plugin_name) else {
        return 0;
    };

    let mut seen_aliases: HashSet<String> =
        report.aliases.iter().map(|alias| alias.name.clone()).collect();
    let mut added = 0usize;
    for (name, value) in aliases {
        if !is_identifierish(name) || !seen_aliases.insert((*name).to_string()) {
            continue;
        }
        report.aliases.push(ImportedAlias {
            name: (*name).to_string(),
            value: (*value).to_string(),
            source_file: None,
            line: None,
            origin: format!("native-plugin:{}", plugin_name),
        });
        added += 1;
    }
    added
}

fn native_plugin_aliases(plugin_name: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match plugin_name {
        "git" => Some(NATIVE_GIT_ALIASES),
        "docker" => Some(NATIVE_DOCKER_ALIASES),
        "kubectl" => Some(NATIVE_KUBECTL_ALIASES),
        "npm" => Some(NATIVE_NPM_ALIASES),
        _ => None,
    }
}

fn apply_native_dynamic_completion_preset(report: &mut ZshImportReport, plugin_name: &str) -> bool {
    let Some((command, args)) = native_dynamic_completion_source(plugin_name) else {
        return false;
    };

    push_dynamic_completion_source(
        report,
        DynamicCompletionSource {
            kind: DynamicCompletionKind::ScriptGenerator,
            command: command.to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            target_shell: "zsh".to_string(),
            source_file: None,
            line: None,
            origin: format!("native-plugin:{}", plugin_name),
        },
    );
    true
}

fn native_dynamic_completion_source(plugin_name: &str) -> Option<(&'static str, &'static [&'static str])> {
    match plugin_name {
        "kubectl" => Some(("kubectl", &["completion", "zsh"])),
        _ => None,
    }
}

fn native_plugin_requires_native_ux(plugin_name: &str) -> bool {
    matches!(
        plugin_name,
        "npm"
            | "alias-finder"
            | "command-not-found"
            | "direnv"
            | "fzf"
            | "thefuck"
            | "zoxide"
            | "zsh-interactive-cd"
            | "zsh-autosuggestions"
            | "zsh-syntax-highlighting"
            | "fast-syntax-highlighting"
            | "zsh-history-substring-search"
            | "fzf-tab"
    )
}

fn native_plugin_widget_preset(plugin_name: &str) -> Option<&'static str> {
    match plugin_name {
        "zsh-autosuggestions" => Some("autosuggestions"),
        "zsh-history-substring-search" => Some("history_substring_search"),
        _ => None,
    }
}

fn native_dynamic_plugin_preset(plugin_name: &str) -> Option<&'static str> {
    match plugin_name {
        "alias-finder" => Some("alias-finder"),
        "command-not-found" => Some("command-not-found"),
        "direnv" => Some("direnv"),
        "fzf" => Some("fzf"),
        "thefuck" => Some("thefuck"),
        "zoxide" => Some("zoxide"),
        "zsh-interactive-cd" => Some("zsh-interactive-cd"),
        _ => None,
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
        || unsupported_features.iter().any(|feature| {
            matches!(
                feature.as_str(),
                "zle" | "bindkey" | "zle-buffer" | "zle-highlighting" | "zsh-hook"
            )
        });

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
    if unsupported_features
        .iter()
        .any(|feature| feature == "dynamic-completion")
    {
        capabilities.push("dynamic_completions_required".to_string());
    }
    if unsupported_features
        .iter()
        .any(|feature| feature == "runtime-completion-provider")
    {
        capabilities.push("runtime_completions_required".to_string());
    }
    if unsupported_features
        .iter()
        .any(|feature| feature == "zsh-completion-function")
    {
        capabilities.push("zsh_completion_function_required".to_string());
    }
    if unsupported_features
        .iter()
        .any(|feature| feature == "zsh-hook")
    {
        capabilities.push("native_lifecycle_hooks_required".to_string());
    }
    if unsupported_features.iter().any(|feature| {
        matches!(
            feature.as_str(),
            "zle" | "bindkey" | "zle-buffer" | "zle-highlighting"
        )
    }) {
        capabilities.push("native_widgets_required".to_string());
    }
    if unsupported_features
        .iter()
        .any(|feature| feature == "autoload")
    {
        capabilities.push("zsh_function_loader_required".to_string());
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
        "alias-finder"
            | "command-not-found"
            | "fzf"
            | "thefuck"
            | "zoxide"
            | "zsh-interactive-cd"
            | "zsh-autosuggestions"
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

fn parse_highlight_style_assignment(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix("ZSH_HIGHLIGHT_STYLES[")?;
    let (key, value) = rest.split_once("]=")?;
    if key.is_empty() || key.contains(']') {
        return None;
    }
    Some((key.to_ascii_lowercase(), unquote(value.trim())))
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

fn is_prompt_assignment(key: &str) -> bool {
    matches!(key, "PROMPT" | "PS1" | "RPROMPT" | "RPS1")
}

fn is_right_prompt_assignment(key: &str) -> bool {
    matches!(key, "RPROMPT" | "RPS1")
}

fn is_git_prompt_assignment(key: &str) -> bool {
    key.starts_with("ZSH_THEME_GIT_PROMPT_")
}

fn record_git_prompt_assignment(
    key: String,
    value: String,
    source_file: Option<&Path>,
    line_no: usize,
    report: &mut ZshImportReport,
    origin: &str,
) {
    let value = decode_prompt_value(&value);
    let translated_value = translate_zsh_prompt_literal(&value);
    let short_key = key
        .strip_prefix("ZSH_THEME_GIT_PROMPT_")
        .unwrap_or(&key)
        .to_ascii_lowercase();

    match short_key.as_str() {
        "prefix" => report.git_prompt.prefix = Some(translated_value.clone()),
        "suffix" => report.git_prompt.suffix = Some(translated_value.clone()),
        "dirty" => report.git_prompt.dirty = Some(translated_value.clone()),
        "clean" => report.git_prompt.clean = Some(translated_value.clone()),
        _ => {}
    }

    report.git_prompt.variables.push(ImportedGitPromptVar {
        key,
        value,
        translated_value: Some(translated_value),
        source_file: source_file.map(Path::to_path_buf),
        line: Some(line_no),
        origin: origin.to_string(),
    });
}

fn record_prompt_assignment(
    key: String,
    value: String,
    source_file: Option<&Path>,
    line_no: usize,
    report: &mut ZshImportReport,
    origin: &str,
) {
    let value = decode_prompt_value(&value);
    let translation = translate_zsh_prompt(&value);
    let imported = ImportedPrompt {
        value,
        translated_format: translation.format,
        unsupported_segments: translation.unsupported_segments,
        source_file: source_file.map(Path::to_path_buf),
        line: Some(line_no),
        origin: origin.to_string(),
    };

    let target = if is_right_prompt_assignment(&key) {
        &mut report.right_prompt
    } else {
        &mut report.prompt
    };
    if origin == "theme" && target.is_some() {
        return;
    }
    *target = Some(imported);
}

fn refresh_prompt_translations(report: &mut ZshImportReport) {
    for prompt in [&mut report.prompt, &mut report.right_prompt]
        .into_iter()
        .flatten()
    {
        let translation = translate_zsh_prompt(&prompt.value);
        prompt.translated_format = translation.format;
        prompt.unsupported_segments = translation.unsupported_segments;
    }
}

pub fn translate_zsh_prompt(value: &str) -> ZshPromptTranslation {
    let decoded = decode_prompt_value(value);
    let chars: Vec<char> = decoded.chars().collect();
    let mut out = String::new();
    let mut unsupported_segments = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        match chars[i] {
            '\\' if matches!(chars.get(i + 1), Some('$'))
                && matches!(chars.get(i + 2), Some('(')) =>
            {
                if let Some((next_i, command)) = command_substitution_at(&chars, i + 1) {
                    if is_git_prompt_info_command(&command) {
                        out.push_str("{git_prompt}");
                    } else {
                        push_unsupported_segment(
                            &mut unsupported_segments,
                            chars_to_string(&chars[i + 1..next_i]),
                        );
                    }
                    i = next_i;
                } else {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            '%' => {
                let Some(next) = chars.get(i + 1).copied() else {
                    out.push('%');
                    i += 1;
                    continue;
                };

                match next {
                    '%' => {
                        out.push('%');
                        i += 2;
                    }
                    '#' => {
                        out.push_str("{symbol}");
                        i += 2;
                    }
                    'n' => {
                        out.push_str("{user}");
                        i += 2;
                    }
                    'm' | 'M' => {
                        out.push_str("{host}");
                        i += 2;
                    }
                    '~' | '/' | 'd' | 'c' | 'C' => {
                        out.push_str("{cwd}");
                        i += 2;
                    }
                    '0'..='9' => {
                        let mut j = i + 1;
                        while chars.get(j).is_some_and(|ch| ch.is_ascii_digit()) {
                            j += 1;
                        }
                        if matches!(chars.get(j), Some('~' | '/' | 'd' | 'c' | 'C')) {
                            out.push_str("{cwd}");
                            i = j + 1;
                        } else {
                            let next_i = (j + 1).min(chars.len());
                            push_unsupported_segment(
                                &mut unsupported_segments,
                                chars_to_string(&chars[i..next_i]),
                            );
                            i = next_i;
                        }
                    }
                    'F' | 'K' => {
                        if matches!(chars.get(i + 2), Some('{')) {
                            i = take_braced(&chars, i + 2).unwrap_or(i + 2);
                        } else {
                            i += 2;
                        }
                    }
                    'f' | 'k' | 'B' | 'b' | 'U' | 'u' | 'S' | 's' | 'E' => {
                        i += 2;
                    }
                    '{' => {
                        i = take_nonprinting_prompt_escape(&chars, i + 2).unwrap_or(i + 2);
                    }
                    '(' => {
                        let next_i = take_balanced(&chars, i + 1, '(', ')').unwrap_or(i + 2);
                        push_unsupported_segment(
                            &mut unsupported_segments,
                            chars_to_string(&chars[i..next_i]),
                        );
                        i = next_i;
                    }
                    'D' => {
                        let next_i = if matches!(chars.get(i + 2), Some('{')) {
                            take_braced(&chars, i + 2).unwrap_or(i + 2)
                        } else {
                            i + 2
                        };
                        push_unsupported_segment(
                            &mut unsupported_segments,
                            chars_to_string(&chars[i..next_i]),
                        );
                        i = next_i;
                    }
                    _ => {
                        let next_i = (i + 2).min(chars.len());
                        push_unsupported_segment(
                            &mut unsupported_segments,
                            chars_to_string(&chars[i..next_i]),
                        );
                        i = next_i;
                    }
                }
            }
            '$' if matches!(chars.get(i + 1), Some('(')) => {
                if let Some((next_i, command)) = command_substitution_at(&chars, i) {
                    if is_git_prompt_info_command(&command) {
                        out.push_str("{git_prompt}");
                    } else {
                        push_unsupported_segment(
                            &mut unsupported_segments,
                            chars_to_string(&chars[i..next_i]),
                        );
                    }
                    i = next_i;
                } else {
                    i += 2;
                }
            }
            '$' if matches!(chars.get(i + 1), Some('{')) => {
                let next_i = take_braced(&chars, i + 1).unwrap_or(i + 2);
                push_unsupported_segment(
                    &mut unsupported_segments,
                    chars_to_string(&chars[i..next_i]),
                );
                i = next_i;
            }
            '$' if chars
                .get(i + 1)
                .is_some_and(|ch| *ch == '_' || ch.is_ascii_alphabetic()) =>
            {
                let next_i = take_variable_like(&chars, i);
                push_unsupported_segment(
                    &mut unsupported_segments,
                    chars_to_string(&chars[i..next_i]),
                );
                i = next_i;
            }
            '`' => {
                let next_i = take_backtick_command(&chars, i).unwrap_or(i + 1);
                let command = chars_to_string(&chars[i + 1..next_i.saturating_sub(1)]);
                if is_git_prompt_info_command(&command) {
                    out.push_str("{git_prompt}");
                } else {
                    push_unsupported_segment(
                        &mut unsupported_segments,
                        chars_to_string(&chars[i..next_i]),
                    );
                }
                i = next_i;
            }
            ch => {
                out.push(ch);
                i += 1;
            }
        }
    }

    let format = clean_prompt_template(&out);
    ZshPromptTranslation {
        format: (!format.trim().is_empty()).then_some(format),
        unsupported_segments,
    }
}

fn translate_zsh_prompt_literal(value: &str) -> String {
    translate_zsh_prompt(value).format.unwrap_or_default()
}

fn decode_prompt_value(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(inner) = trimmed.strip_prefix("$'").and_then(|v| v.strip_suffix('\'')) {
        return decode_c_style_escapes(inner);
    }
    if let Some(inner) = trimmed.strip_prefix("$\"").and_then(|v| v.strip_suffix('"')) {
        return decode_c_style_escapes(inner);
    }
    let decoded = if is_wrapped_quote(trimmed) {
        unquote(trimmed)
    } else {
        value.to_string()
    };
    decode_c_style_escapes(&decoded)
}

fn is_wrapped_quote(value: &str) -> bool {
    value.len() >= 2
        && ((value.starts_with('\'') && value.ends_with('\''))
            || (value.starts_with('"') && value.ends_with('"')))
}

fn decode_c_style_escapes(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('a') => out.push('\x07'),
            Some('b') => out.push('\x08'),
            Some('e') | Some('E') => out.push('\x1b'),
            Some('\\') => out.push('\\'),
            Some('\'') => out.push('\''),
            Some('"') => out.push('"'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn take_braced(chars: &[char], open_index: usize) -> Option<usize> {
    take_balanced(chars, open_index, '{', '}')
}

fn take_nonprinting_prompt_escape(chars: &[char], start_index: usize) -> Option<usize> {
    let mut i = start_index;
    while i + 1 < chars.len() {
        if chars[i] == '%' && chars[i + 1] == '}' {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}

fn command_substitution_at(chars: &[char], dollar_index: usize) -> Option<(usize, String)> {
    if !matches!(chars.get(dollar_index), Some('$'))
        || !matches!(chars.get(dollar_index + 1), Some('('))
    {
        return None;
    }
    let next_i = take_balanced(chars, dollar_index + 1, '(', ')')?;
    let command = chars_to_string(&chars[dollar_index + 2..next_i.saturating_sub(1)])
        .trim()
        .to_string();
    Some((next_i, command))
}

fn is_git_prompt_info_command(command: &str) -> bool {
    command.trim() == "git_prompt_info"
}

fn take_balanced(chars: &[char], open_index: usize, open: char, close: char) -> Option<usize> {
    if chars.get(open_index).copied() != Some(open) {
        return None;
    }

    let mut depth = 0usize;
    let mut single = false;
    let mut double = false;
    let mut escaped = false;

    for (offset, ch) in chars[open_index..].iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match ch {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            ch if ch == open && !single && !double => depth += 1,
            ch if ch == close && !single && !double => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open_index + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn take_variable_like(chars: &[char], dollar_index: usize) -> usize {
    let mut i = dollar_index + 1;
    while chars
        .get(i)
        .is_some_and(|ch| *ch == '_' || ch.is_ascii_alphanumeric())
    {
        i += 1;
    }
    i
}

fn take_backtick_command(chars: &[char], start_index: usize) -> Option<usize> {
    let mut escaped = false;
    for i in start_index + 1..chars.len() {
        if escaped {
            escaped = false;
            continue;
        }
        if chars[i] == '\\' {
            escaped = true;
            continue;
        }
        if chars[i] == '`' {
            return Some(i + 1);
        }
    }
    None
}

fn chars_to_string(chars: &[char]) -> String {
    chars.iter().collect()
}

fn push_unsupported_segment(segments: &mut Vec<String>, segment: String) {
    if segment.trim().is_empty() || segments.iter().any(|existing| existing == &segment) {
        return;
    }
    segments.push(segment);
}

fn clean_prompt_template(value: &str) -> String {
    let mut out = String::new();
    let mut previous_space = false;
    for ch in value.chars() {
        if ch == ' ' {
            if previous_space {
                continue;
            }
            previous_space = true;
        } else {
            previous_space = false;
        }
        out.push(ch);
    }
    out
}

fn plugin_names_for_import_plan(
    options: &ZshImportOptions,
    report: &ZshImportReport,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for name in report
        .plugins
        .iter()
        .map(|plugin| plugin.name.as_str())
        .chain(options.plugins.iter().map(String::as_str))
    {
        if !name.is_empty() && seen.insert(name.to_string()) {
            names.push(name.to_string());
        }
    }
    names.sort();
    names
}

fn aliases_for_import_plan(report: &ZshImportReport) -> Vec<(String, String)> {
    let mut aliases: HashMap<String, String> = HashMap::new();
    for alias in &report.aliases {
        if is_identifierish(&alias.name) {
            aliases.insert(alias.name.clone(), alias.value.clone());
        }
    }
    let mut aliases: Vec<(String, String)> = aliases.into_iter().collect();
    aliases.sort_by(|left, right| left.0.cmp(&right.0));
    aliases
}

fn dynamic_completion_commands_for_import_plan(report: &ZshImportReport) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut commands = Vec::new();
    for source in &report.dynamic_completion_sources {
        if source.kind == DynamicCompletionKind::ScriptGenerator
            && source.target_shell == "zsh"
            && is_safe_name(&source.command)
            && seen.insert(source.command.clone())
        {
            commands.push(source.command.clone());
        }
    }
    commands.sort();
    commands
}

fn runtime_completion_commands_for_import_plan(report: &ZshImportReport) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut commands = Vec::new();
    for source in &report.dynamic_completion_sources {
        if source.kind == DynamicCompletionKind::RuntimeProvider
            && is_safe_name(&source.command)
            && seen.insert(source.command.clone())
        {
            commands.push(source.command.clone());
        }
    }
    commands.sort();
    commands
}

fn native_hook_todos_for_import_plan(report: &ZshImportReport, hook: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut scripts = Vec::new();
    for suggestion in &report.native_hooks {
        if suggestion.hook == hook && seen.insert(suggestion.function.clone()) {
            scripts.push(format!(
                "# TODO translate zsh hook function: {}",
                suggestion.function
            ));
        }
    }
    scripts.sort();
    scripts
}

fn native_widget_todos_for_import_plan(report: &ZshImportReport) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut todos = Vec::new();
    for suggestion in &report.native_widgets {
        let todo = match (&suggestion.function, &suggestion.keymap, &suggestion.key) {
            (Some(function), _, None) => format!(
                "TODO native widget: {} -> {}",
                suggestion.widget, function
            ),
            (_, Some(keymap), Some(key)) => format!(
                "TODO native keybinding: {} {} -> {}",
                keymap, key, suggestion.widget
            ),
            (_, None, Some(key)) => {
                format!("TODO native keybinding: {} -> {}", key, suggestion.widget)
            }
            _ => format!("TODO native widget: {}", suggestion.widget),
        };
        if seen.insert(todo.clone()) {
            todos.push(todo);
        }
    }
    todos.sort();
    todos
}

fn zsh_function_todos_for_import_plan(report: &ZshImportReport) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut todos = Vec::new();
    for suggestion in &report.zsh_functions {
        let todo = format!(
            "TODO native function/helper: {} kind={} autoloaded={}",
            suggestion.function,
            zsh_function_kind_name(suggestion.kind),
            suggestion.autoloaded
        );
        if seen.insert(todo.clone()) {
            todos.push(todo);
        }
    }
    todos.sort();
    todos
}

fn native_widget_presets_for_import_plan(report: &ZshImportReport) -> Vec<String> {
    let mut presets = HashSet::new();
    for plugin in &report.plugins {
        if let Some(preset) = native_plugin_widget_preset(&plugin.name) {
            presets.insert(preset.to_string());
        }
    }
    for suggestion in &report.native_widgets {
        if suggestion.widget.starts_with("autosuggest-") {
            presets.insert("autosuggestions".to_string());
        }
        if suggestion.widget.starts_with("history-substring-search-") {
            presets.insert("history_substring_search".to_string());
        }
    }
    let mut presets: Vec<String> = presets.into_iter().collect();
    presets.sort();
    presets
}

fn native_plugin_presets_for_import_plan(report: &ZshImportReport) -> Vec<String> {
    let mut presets = HashSet::new();
    for plugin in &report.plugins {
        if let Some(preset) = native_dynamic_plugin_preset(&plugin.name) {
            presets.insert(preset.to_string());
        }
    }
    for hook in &report.native_hooks {
        if hook.function == "_direnv_hook" {
            presets.insert("direnv".to_string());
        }
        if hook.function == "preexec_alias-finder" || hook.function == "alias-finder" {
            presets.insert("alias-finder".to_string());
        }
    }
    for function in &report.zsh_functions {
        if function.function == "_direnv_hook" {
            presets.insert("direnv".to_string());
        }
        if function.function == "preexec_alias-finder" || function.function == "alias-finder" {
            presets.insert("alias-finder".to_string());
        }
    }
    let mut presets: Vec<String> = presets.into_iter().collect();
    presets.sort();
    presets
}

fn dynamic_completion_script_generator_count(report: &ZshImportReport) -> usize {
    report
        .dynamic_completion_sources
        .iter()
        .filter(|source| source.kind == DynamicCompletionKind::ScriptGenerator)
        .count()
}

fn dynamic_completion_runtime_provider_count(report: &ZshImportReport) -> usize {
    report
        .dynamic_completion_sources
        .iter()
        .filter(|source| source.kind == DynamicCompletionKind::RuntimeProvider)
        .count()
}

fn compat_level_name(level: ZshCompatLevel) -> &'static str {
    match level {
        ZshCompatLevel::Safe => "safe",
        ZshCompatLevel::Warn => "warn",
        ZshCompatLevel::Experimental => "experimental",
    }
}

fn toml_array(values: &[String]) -> String {
    let items = values
        .iter()
        .map(|value| toml_quote(value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{}]", items)
}

fn toml_quote(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push(' '),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn managed_import_block(plan: &str) -> String {
    format!(
        "{}\n{}\n{}\n",
        ZSH_IMPORT_BLOCK_START,
        plan.trim(),
        ZSH_IMPORT_BLOCK_END
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ZshImportApplyPreview {
    next_config: String,
    replaced_existing_block: bool,
}

fn preview_import_plan_update(
    original: &str,
    plan: &str,
) -> anyhow::Result<ZshImportApplyPreview> {
    let block = managed_import_block(plan);
    let (next_config, replaced_existing_block) =
        replace_managed_import_block(original, &block)?;
    toml::from_str::<toml::Value>(&next_config).with_context(|| {
        "generated zsh import block would make ~/.winshrc.toml invalid; run \
         --zsh-compat-import-plan and merge manually"
    })?;

    Ok(ZshImportApplyPreview {
        next_config,
        replaced_existing_block,
    })
}

fn replace_managed_import_block(
    original: &str,
    block: &str,
) -> anyhow::Result<(String, bool)> {
    let Some((start, end)) = managed_import_block_range(original)? else {
        let mut next = original.to_string();
        if !next.trim().is_empty() {
            if !next.ends_with('\n') {
                next.push('\n');
            }
            next.push('\n');
        }
        next.push_str(block);
        return Ok((next, false));
    };

    let mut next = String::new();
    next.push_str(&original[..start]);
    next.push_str(block);
    next.push_str(&original[end..]);
    Ok((next, true))
}

fn managed_import_block_state(original: &str) -> ZshImportBlockState {
    match managed_import_block_range(original) {
        Ok(Some(_)) => ZshImportBlockState::Present,
        Ok(None) => ZshImportBlockState::Missing,
        Err(_) => ZshImportBlockState::Malformed,
    }
}

fn managed_import_block_range(original: &str) -> anyhow::Result<Option<(usize, usize)>> {
    let start_count = original.matches(ZSH_IMPORT_BLOCK_START).count();
    let end_count = original.matches(ZSH_IMPORT_BLOCK_END).count();

    if start_count == 0 && end_count == 0 {
        return Ok(None);
    }

    if start_count != 1 || end_count != 1 {
        return Err(anyhow!(
            "found malformed winuxsh-managed zsh import block markers in ~/.winshrc.toml"
        ));
    }

    let start = original
        .find(ZSH_IMPORT_BLOCK_START)
        .expect("counted one start marker");
    let after_start = start + ZSH_IMPORT_BLOCK_START.len();
    let Some(end_relative) = original[after_start..].find(ZSH_IMPORT_BLOCK_END) else {
        return Err(anyhow!(
            "found zsh import block start marker without matching end marker in ~/.winshrc.toml"
        ));
    };
    let end = after_start + end_relative + ZSH_IMPORT_BLOCK_END.len();

    Ok(Some((start, end)))
}

fn backup_path_for(config_path: &Path, suffix: &str) -> PathBuf {
    let file_name = config_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(".winshrc.toml");
    config_path.with_file_name(format!("{}.{}.bak", file_name, suffix))
}

fn unique_backup_path_for(config_path: &Path, suffix: &str) -> PathBuf {
    let mut backup_path = backup_path_for(config_path, suffix);
    let mut attempt = 1usize;
    while backup_path.exists() {
        backup_path = backup_path_for(config_path, &format!("{}-{}", suffix, attempt));
        attempt += 1;
    }
    backup_path
}

fn backup_paths_for(config_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let Some(parent) = config_path.parent() else {
        return Ok(Vec::new());
    };
    let file_name = config_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(".winshrc.toml");
    let prefix = format!("{}.", file_name);

    let mut paths = Vec::new();
    match std::fs::read_dir(parent) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.with_context(|| {
                    format!("failed to inspect backup files in {}", parent.display())
                })?;
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if name.starts_with(&prefix) && name.ends_with(".bak") {
                    paths.push(path);
                }
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to inspect {}", parent.display()))
        }
    }

    paths.sort();
    Ok(paths)
}

fn powershell_single_quote_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\'', "''");
    format!("'{}'", value)
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn diagnostic_count(report: &ZshImportReport, severity: DiagnosticSeverity) -> usize {
    report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == severity)
        .count()
}

fn plugin_tier_count(report: &ZshImportReport, tier: PluginImportTier) -> usize {
    report
        .plugins
        .iter()
        .filter(|plugin| plugin.tier == tier)
        .count()
}

fn block_state_label(state: ZshImportBlockState) -> &'static str {
    match state {
        ZshImportBlockState::Missing => "missing",
        ZshImportBlockState::Present => "present",
        ZshImportBlockState::Malformed => "malformed",
    }
}

fn apply_readiness_label(readiness: ZshImportApplyReadiness) -> &'static str {
    match readiness {
        ZshImportApplyReadiness::AddNewBlock => "ready (add new block)",
        ZshImportApplyReadiness::ReplaceExistingBlock => "ready (replace existing block)",
        ZshImportApplyReadiness::Blocked => "blocked",
    }
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
        | "ZSH_HIGHLIGHT_STYLES" | "ZSH_HIGHLIGHT_HIGHLIGHTERS"
        | "ZSH_HIGHLIGHT_MAXLENGTH" => {
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
        (
            "add-zsh-hook",
            "zsh-hook",
            "zsh hook plugins require native lifecycle hooks",
        ),
        (
            "precmd_functions",
            "zsh-hook",
            "precmd hooks require native lifecycle hooks",
        ),
        (
            "preexec_functions",
            "zsh-hook",
            "preexec hooks require native lifecycle hooks",
        ),
        (
            "chpwd_functions",
            "zsh-hook",
            "chpwd hooks require native lifecycle hooks",
        ),
        (
            "autoload ",
            "autoload",
            "autoloaded zsh functions are not executed",
        ),
        (
            "autoload\t",
            "autoload",
            "autoloaded zsh functions are not executed",
        ),
        ("BUFFER", "zle-buffer", "BUFFER/CURSOR style plugins are not executed"),
        ("CURSOR", "zle-buffer", "BUFFER/CURSOR style plugins are not executed"),
        (
            "region_highlight",
            "zle-highlighting",
            "region_highlight maps to native reedline highlighting",
        ),
        (
            "compadd",
            "zsh-completion-function",
            "zsh completion functions require native completion providers",
        ),
        (
            "_describe",
            "zsh-completion-function",
            "zsh completion functions require native completion providers",
        ),
        (
            "_values",
            "zsh-completion-function",
            "zsh completion functions require native completion providers",
        ),
        (
            "_wanted",
            "zsh-completion-function",
            "zsh completion functions require native completion providers",
        ),
        (
            "_comps[",
            "zsh-completion-function",
            "zsh completion function registration is not executed",
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

    if is_zsh_hook_function(line) {
        report.diagnostics.push(ZshCompatDiagnostic {
            severity: DiagnosticSeverity::Unsupported,
            feature: "zsh-hook".to_string(),
            message: "zsh lifecycle hooks require native lifecycle hooks".to_string(),
            source_file: source_file.map(Path::to_path_buf),
            line: Some(line_no),
        });
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

fn is_zsh_hook_function(line: &str) -> bool {
    zsh_hook_function_name(line).is_some()
}

fn parse_native_hook_suggestions(
    line: &str,
    source_file: Option<&Path>,
    line_no: usize,
    mode: ScanMode,
) -> Vec<NativeHookSuggestion> {
    let mut suggestions = Vec::new();

    if let Some((hook, function)) = parse_add_zsh_hook(line) {
        suggestions.push(native_hook_suggestion(
            hook,
            function,
            source_file,
            line_no,
            mode,
        ));
    }

    for (hook, function) in parse_hook_functions_array(line) {
        suggestions.push(native_hook_suggestion(
            hook,
            function,
            source_file,
            line_no,
            mode,
        ));
    }

    if let Some(hook) = zsh_hook_function_name(line) {
        suggestions.push(native_hook_suggestion(
            hook.to_string(),
            hook.to_string(),
            source_file,
            line_no,
            mode,
        ));
    }

    suggestions
}

fn native_hook_suggestion(
    hook: String,
    function: String,
    source_file: Option<&Path>,
    line_no: usize,
    mode: ScanMode,
) -> NativeHookSuggestion {
    NativeHookSuggestion {
        hook,
        function,
        source_file: source_file.map(Path::to_path_buf),
        line: Some(line_no),
        origin: scan_mode_origin(mode).to_string(),
    }
}

fn parse_add_zsh_hook(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("add-zsh-hook ")?;
    let words = split_shell_words(rest);
    let mut positional = words.iter().filter(|word| !word.starts_with('-'));
    let hook = positional.next()?.to_string();
    let function = positional.next()?.to_string();
    if is_zsh_hook_name(&hook) && is_safe_hook_function_name(&function) {
        Some((hook, function))
    } else {
        None
    }
}

fn parse_hook_functions_array(line: &str) -> Vec<(String, String)> {
    let Some((array_name, value)) = line.split_once('=') else {
        return Vec::new();
    };
    let array_name = array_name.trim().strip_suffix('+').unwrap_or(array_name.trim());
    let (hook, array_name) = match array_name {
        "precmd_functions" => ("precmd", "precmd_functions"),
        "preexec_functions" => ("preexec", "preexec_functions"),
        "chpwd_functions" => ("chpwd", "chpwd_functions"),
        _ => return Vec::new(),
    };
    if !line.trim_start().starts_with(array_name) {
        return Vec::new();
    }
    let value = value.trim();
    let Some(inner) = value.strip_prefix('(').and_then(|value| value.strip_suffix(')')) else {
        return Vec::new();
    };
    split_shell_words(inner)
        .into_iter()
        .filter(|function| is_safe_hook_function_name(function))
        .map(|function| (hook.to_string(), function))
        .collect()
}

fn zsh_hook_function_name(line: &str) -> Option<&'static str> {
    for hook in ["precmd", "preexec", "chpwd"] {
        if line.starts_with(&format!("{hook}()"))
            || line.starts_with(&format!("{hook} ()"))
            || line.starts_with(&format!("function {hook}"))
        {
            return Some(hook);
        }
    }
    None
}

fn is_zsh_hook_name(value: &str) -> bool {
    matches!(value, "precmd" | "preexec" | "chpwd")
}

fn is_safe_hook_function_name(value: &str) -> bool {
    is_safe_name(value) && !value.starts_with('-') && !value.contains('/')
}

fn push_native_hook_suggestion(report: &mut ZshImportReport, suggestion: NativeHookSuggestion) {
    if report.native_hooks.iter().any(|existing| {
        existing.hook == suggestion.hook
            && existing.function == suggestion.function
            && existing.origin == suggestion.origin
    }) {
        return;
    }
    report.native_hooks.push(suggestion);
}

fn parse_native_widget_suggestions(
    line: &str,
    source_file: Option<&Path>,
    line_no: usize,
    mode: ScanMode,
) -> Vec<NativeWidgetSuggestion> {
    let mut suggestions = Vec::new();

    if let Some((widget, function)) = parse_zle_widget_registration(line) {
        suggestions.push(native_widget_suggestion(
            widget,
            function,
            None,
            None,
            source_file,
            line_no,
            mode,
        ));
    }

    if let Some((keymap, key, widget)) = parse_bindkey_widget_binding(line) {
        suggestions.push(native_widget_suggestion(
            widget,
            None,
            Some(key),
            keymap,
            source_file,
            line_no,
            mode,
        ));
    }

    suggestions
}

fn native_widget_suggestion(
    widget: String,
    function: Option<String>,
    key: Option<String>,
    keymap: Option<String>,
    source_file: Option<&Path>,
    line_no: usize,
    mode: ScanMode,
) -> NativeWidgetSuggestion {
    NativeWidgetSuggestion {
        widget,
        function,
        key,
        keymap,
        source_file: source_file.map(Path::to_path_buf),
        line: Some(line_no),
        origin: scan_mode_origin(mode).to_string(),
    }
}

fn parse_zle_widget_registration(line: &str) -> Option<(String, Option<String>)> {
    let words = split_shell_words(line);
    if words.first().map_or(true, |word| word != "zle") {
        return None;
    }

    let marker_idx = words.iter().position(|word| word == "-N" || word == "-C")?;
    match words.get(marker_idx).map(String::as_str) {
        Some("-N") => {
            let widget = words.get(marker_idx + 1)?;
            if !is_safe_zle_name(widget) {
                return None;
            }
            let function = words
                .get(marker_idx + 2)
                .filter(|function| is_safe_zle_name(function))
                .cloned();
            Some((widget.clone(), function))
        }
        Some("-C") => {
            let widget = words.get(marker_idx + 1)?;
            let function = words.get(marker_idx + 3)?;
            if is_safe_zle_name(widget) && is_safe_zle_name(function) {
                Some((widget.clone(), Some(function.clone())))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_bindkey_widget_binding(line: &str) -> Option<(Option<String>, String, String)> {
    let words = split_shell_words(line);
    if words.first().map_or(true, |word| word != "bindkey") {
        return None;
    }
    if words
        .iter()
        .any(|word| matches!(word.as_str(), "-e" | "-v" | "-r" | "-s"))
    {
        return None;
    }

    let mut idx = 1usize;
    let mut keymap = None;
    while idx < words.len() {
        match words[idx].as_str() {
            "-M" => {
                let map = words.get(idx + 1)?;
                if !is_safe_zle_name(map) {
                    return None;
                }
                keymap = Some(map.clone());
                idx += 2;
            }
            option if option.starts_with('-') => return None,
            _ => break,
        }
    }

    let key = words.get(idx)?;
    let widget = words.get(idx + 1)?;
    if words.get(idx + 2).is_some() {
        return None;
    }
    if is_safe_key_sequence(key) && is_safe_zle_name(widget) {
        Some((keymap, key.clone(), widget.clone()))
    } else {
        None
    }
}

fn is_safe_zle_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch == '_' || ch == '-' || ch == '.' || ch == ':' || ch.is_ascii_alphanumeric())
}

fn is_safe_key_sequence(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r'))
}

fn push_native_widget_suggestion(report: &mut ZshImportReport, suggestion: NativeWidgetSuggestion) {
    if report.native_widgets.iter().any(|existing| {
        existing.widget == suggestion.widget
            && existing.function == suggestion.function
            && existing.key == suggestion.key
            && existing.keymap == suggestion.keymap
            && existing.origin == suggestion.origin
    }) {
        return;
    }
    report.native_widgets.push(suggestion);
}

fn parse_zsh_function_suggestions(
    line: &str,
    source_file: Option<&Path>,
    line_no: usize,
    mode: ScanMode,
) -> Vec<ZshFunctionSuggestion> {
    let mut suggestions = Vec::new();

    for function in parse_autoload_functions(line) {
        suggestions.push(zsh_function_suggestion(
            function,
            true,
            source_file,
            line_no,
            mode,
        ));
    }

    if let Some(function) = parse_zsh_function_definition(line) {
        suggestions.push(zsh_function_suggestion(
            function,
            false,
            source_file,
            line_no,
            mode,
        ));
    }

    suggestions
}

fn zsh_function_suggestion(
    function: String,
    autoloaded: bool,
    source_file: Option<&Path>,
    line_no: usize,
    mode: ScanMode,
) -> ZshFunctionSuggestion {
    ZshFunctionSuggestion {
        kind: classify_zsh_function(&function),
        function,
        autoloaded,
        source_file: source_file.map(Path::to_path_buf),
        line: Some(line_no),
        origin: scan_mode_origin(mode).to_string(),
    }
}

fn parse_autoload_functions(line: &str) -> Vec<String> {
    let words = split_shell_words(line);
    if words.first().map_or(true, |word| word != "autoload") {
        return Vec::new();
    }

    let mut functions = Vec::new();
    for raw in words.into_iter().skip(1) {
        if matches!(raw.as_str(), "&&" | "||" | "&" | "|") {
            break;
        }
        let stop_after = raw.ends_with(';');
        let word = raw.trim_end_matches(';');
        if word.starts_with('-') || word.starts_with('+') || word.is_empty() {
            if stop_after {
                break;
            }
            continue;
        }
        if is_safe_zsh_function_name(word) {
            functions.push(word.to_string());
        }
        if stop_after {
            break;
        }
    }
    functions
}

fn parse_zsh_function_definition(line: &str) -> Option<String> {
    let line = line.trim_start();
    let name = if let Some(rest) = line.strip_prefix("function ") {
        rest.trim_start()
            .chars()
            .take_while(|ch| !ch.is_whitespace() && *ch != '(' && *ch != '{')
            .collect::<String>()
    } else {
        let (name, rest) = line.split_once('(')?;
        let name = name.trim();
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return None;
        }
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(')')?.trim_start();
        if !(rest.is_empty() || rest.starts_with('{')) {
            return None;
        }
        name.to_string()
    };

    if is_safe_zsh_function_name(&name) {
        Some(name)
    } else {
        None
    }
}

fn classify_zsh_function(function: &str) -> ZshFunctionKind {
    let lower = function.to_ascii_lowercase();
    if lower.contains("autosuggest")
        || lower.contains("history-substring")
        || lower.contains("widget")
        || lower.starts_with("_zsh_highlight_widget_")
    {
        return ZshFunctionKind::WidgetHelper;
    }
    if function.starts_with('_') {
        return ZshFunctionKind::CompletionHelper;
    }
    if function == "add-zsh-hook"
        || is_zsh_hook_name(function)
        || lower.starts_with("precmd")
        || lower.starts_with("preexec")
        || lower.starts_with("chpwd")
    {
        return ZshFunctionKind::LifecycleHelper;
    }
    if lower.contains("prompt") {
        return ZshFunctionKind::PromptHelper;
    }
    ZshFunctionKind::GenericHelper
}

fn zsh_function_kind_name(kind: ZshFunctionKind) -> &'static str {
    match kind {
        ZshFunctionKind::CompletionHelper => "completion_helper",
        ZshFunctionKind::LifecycleHelper => "lifecycle_helper",
        ZshFunctionKind::WidgetHelper => "widget_helper",
        ZshFunctionKind::PromptHelper => "prompt_helper",
        ZshFunctionKind::GenericHelper => "generic_helper",
    }
}

fn is_safe_zsh_function_name(value: &str) -> bool {
    is_safe_name(value) && !value.starts_with('-') && !value.contains('/')
}

fn push_zsh_function_suggestion(report: &mut ZshImportReport, suggestion: ZshFunctionSuggestion) {
    if report.zsh_functions.iter().any(|existing| {
        existing.function == suggestion.function
            && existing.kind == suggestion.kind
            && existing.autoloaded == suggestion.autoloaded
            && existing.origin == suggestion.origin
    }) {
        return;
    }
    report.zsh_functions.push(suggestion);
}

fn push_dynamic_completion_source(report: &mut ZshImportReport, source: DynamicCompletionSource) {
    if report.dynamic_completion_sources.iter().any(|existing| {
        existing.kind == source.kind
            && existing.command == source.command
            && existing.args == source.args
            && existing.target_shell == source.target_shell
            && existing.source_file == source.source_file
            && existing.line == source.line
    }) {
        return;
    }
    report.dynamic_completion_sources.push(source);
}

fn parse_dynamic_completion_source(
    line: &str,
) -> Option<(String, Vec<String>, String, DynamicCompletionKind)> {
    let candidate = process_substitution_command(line).unwrap_or_else(|| line.to_string());
    let prefix = command_prefix_before_control(&candidate);
    let words = split_shell_words(&prefix);
    if words.len() < 3 {
        return None;
    }

    for idx in 0..words.len().saturating_sub(1) {
        if words[idx] == "completion" && words[idx + 1] == "zsh" {
            let mut command_idx = 0usize;
            while command_idx < idx && is_inline_env_assignment(&words[command_idx]) {
                command_idx += 1;
            }
            if words.get(command_idx).is_some_and(|word| word == "command") {
                command_idx += 1;
            }
            if command_idx >= idx {
                return None;
            }
            let command = words[command_idx].clone();
            if !is_safe_name(&command) {
                return None;
            }
            let args = words[command_idx + 1..=idx + 1].to_vec();
            return Some((
                command,
                args,
                "zsh".to_string(),
                DynamicCompletionKind::ScriptGenerator,
            ));
        }
    }

    for idx in 0..words.len().saturating_sub(1) {
        if words[idx] == "completion" && words[idx + 1] == "--" {
            for command_idx in (0..idx).rev() {
                let command = words[command_idx].trim_start_matches("$(");
                if command == "command" || is_inline_env_assignment(command) {
                    continue;
                }
                if !is_safe_name(command) {
                    continue;
                }
                let args = words[command_idx + 1..=idx + 1].to_vec();
                return Some((
                    command.to_string(),
                    args,
                    "words".to_string(),
                    DynamicCompletionKind::RuntimeProvider,
                ));
            }
        }
    }

    None
}

fn dynamic_completion_feature(kind: DynamicCompletionKind) -> &'static str {
    match kind {
        DynamicCompletionKind::ScriptGenerator => "dynamic-completion",
        DynamicCompletionKind::RuntimeProvider => "runtime-completion-provider",
    }
}

fn dynamic_completion_message(
    kind: DynamicCompletionKind,
    command: &str,
    args: &[String],
) -> String {
    match kind {
        DynamicCompletionKind::ScriptGenerator => format!(
            "dynamic completion generator detected: {} {}",
            command,
            args.join(" ")
        ),
        DynamicCompletionKind::RuntimeProvider => format!(
            "runtime completion provider detected: {} {}",
            command,
            args.join(" ")
        ),
    }
}

fn process_substitution_command(line: &str) -> Option<String> {
    let start = line.find("<(")?;
    let after_start = start + 2;
    let end = line[after_start..].rfind(')')? + after_start;
    let inner = line[after_start..end].trim();
    (!inner.is_empty()).then(|| inner.to_string())
}

fn command_prefix_before_control(line: &str) -> String {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    let mut out = String::new();

    for ch in line.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !single => {
                out.push(ch);
                escaped = true;
            }
            '\'' if !double => {
                single = !single;
                out.push(ch);
            }
            '"' if !single => {
                double = !double;
                out.push(ch);
            }
            '|' | '>' if !single && !double => break,
            _ => out.push(ch),
        }
    }

    out.trim().to_string()
}

fn is_inline_env_assignment(word: &str) -> bool {
    let Some((key, _)) = word.split_once('=') else {
        return false;
    };
    is_env_identifier(key)
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
    let mut candidate = raw.trim().trim_matches('"').trim_matches('\'');
    if let Some((left, _)) = candidate.split_once('=') {
        candidate = left;
    }
    if let Some((left, _)) = candidate.split_once(':') {
        candidate = left;
    }
    let candidate = candidate.trim();

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
