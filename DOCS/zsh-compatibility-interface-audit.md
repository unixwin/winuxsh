---
tags: [winuxsh, zsh, compatibility, architecture, feasibility]
created: 2026-07-17
status: active
---

# Zsh Compatibility Interface Audit

> Purpose: identify the concrete winuxsh interfaces where zsh / Oh My Zsh
> compatibility can attach, estimate feasibility, and keep the implementation
> path aligned with the Windows-native rubash + winuxcmd + reedline architecture.

## Current Conclusion

Zsh compatibility is feasible if it is implemented as a **safe scanner and
native translator layer**, not as direct zsh script execution.

The best first implementation is:

1. Add a `[zsh]` config section in `.winshrc.toml`.
2. Add a `zsh_compat` runtime module that scans `.zshrc` / Oh My Zsh layout and
   produces a structured import report.
3. Apply only safe records during `Shell::new()`:
   - env/PATH before `rubash::Executor::new()`
   - aliases after executor construction
   - completion dirs/assets before completion state loads
   - prompt/theme/editor settings before REPL starts
4. Add a CLI report path first, such as `winuxsh --zsh-compat-report`, before
   enabling any automatic startup import.

This keeps non-interactive agent mode deterministic and avoids making winuxsh a
zsh interpreter.

## Reference Snapshot

Reference repos are outside winuxsh under `%TEMP%/winuxsh-reference` and must
not be vendored:

- zsh: `62103851e`
- Nushell: `eef1ddd`
- Oh My Zsh: `677a4592`
- zsh-autosuggestions: `85919cd`
- zsh-syntax-highlighting: `1d85c69`

## Oh My Zsh Plugin Scan

Read-only scan of `%TEMP%/winuxsh-reference/ohmyzsh/plugins`:

| Metric | Count |
| --- | ---: |
| Total plugin directories | 357 |
| Plugins with `_cmd` completion files | 91 |
| Plugins with plugin scripts | 321 |
| Plugins containing aliases | 146 |
| Plugins containing `compdef` / `#compdef` | 141 |
| Plugins containing `zstyle` | 32 |
| Plugins touching ZLE-ish APIs | 34 |
| Plugins touching `zmodload` | 15 |
| Plugins touching `zpty` in Oh My Zsh tree | 0 |

The zsh-autosuggestions reference itself uses ZLE, `BUFFER`, `CURSOR`,
`POSTDISPLAY`, `region_highlight`, `zmodload`, and `zpty`. The
zsh-syntax-highlighting reference uses `region_highlight`, `BUFFER`, `CURSOR`,
`zle`, and `zmodload`. These must be native reedline/rubash implementations,
not sourced scripts.

## Existing Winuxsh Attach Points

### Config

File: `crates/winuxsh-runtime/src/config.rs`

Current `FullConfig` already centralizes:

- prompt config
- editor mode
- theme name
- aliases
- completion dirs
- configured winuxcmd path

Feasible extension:

```toml
[zsh]
enabled = false
zdotdir = "~"
import_zshrc = true
import_oh_my_zsh = true
plugins = []
compat_level = "safe"
auto_apply = false
```

Recommended Rust types:

- `ZshConfig`
- `ZshCompatLevel`
- `ZshImportMode` or `auto_apply: bool`

Difficulty: **easy**.

### CLI

File: `src/main.rs`

Current CLI is manual argument matching:

- no args -> REPL
- `-c` -> whole-script execution
- script path -> whole-script execution
- `--help`, `--version`

Feasible extension:

- `winuxsh --zsh-compat-report`
- `winuxsh --zsh-compat-report --json`
- later: `winuxsh --import-zsh --dry-run`

This can be added without `clap` for now, but the manual parser will start to
feel cramped after one or two more commands.

Difficulty: **easy** for report mode, **moderate** if the CLI grows into a
multi-command config/import surface.

### Shell Initialization

File: `crates/winuxsh-runtime/src/shell.rs`

Current order:

1. load native config
2. inject winuxcmd PATH
3. construct `rubash::Executor`
4. apply aliases
5. build prompt/theme
6. set history path
7. build completion state
8. load completion dirs

Zsh compatibility needs a careful ordering:

1. load native config
2. scan zsh profile/layout if enabled or report requested
3. apply safe env/PATH changes to process env
4. inject winuxcmd PATH so winuxcmd still wins
5. construct `rubash::Executor`
6. apply native aliases + imported aliases
7. build prompt/theme using native config + safe zsh theme hints
8. load native completion dirs + imported safe completion dirs/assets

The critical rule: env/PATH imports must happen before `Executor::new()` because
rubash snapshots process env into its own `env_vars`.

Difficulty: **moderate** because the order is important, but the hook points are
already clean.

### Completion

Files:

- `crates/winuxsh-runtime/src/completion/external.rs`
- `crates/winuxsh-runtime/src/completion/bash_import.rs`
- `crates/winuxsh-runtime/src/completion/completer.rs`

Current model already has the right shape:

- `CommandDef`
- `FlagDef`
- `SubcommandDef`
- TOML definitions
- built-in defaults
- user override priority
- bash completion import/cache

Feasible zsh additions:

- discover `_cmd` files in `fpath` / Oh My Zsh plugin dirs
- parse `#compdef foo bar`
- parse simple `compdef _git g=git`
- parse a small `_arguments` subset into `CommandDef`
- mark dynamic/complex completions unsupported with diagnostics

Difficulty:

- `#compdef` and simple `compdef`: **easy**
- completion asset discovery and ordering: **moderate**
- `_arguments` subset: **hard**
- arbitrary dynamic completion functions: **defer**

### REPL / Reedline

File: `crates/winuxsh-runtime/src/repl.rs`

Reedline 0.33 already exposes:

- `with_hinter(Box<dyn Hinter>)`
- `with_highlighter(Box<dyn Highlighter>)`
- `Prompt::render_prompt_right`
- `Prompt::render_prompt_indicator`

This makes native zsh-like features feasible:

- history autosuggestions via `DefaultHinter` or a custom `Hinter`
- syntax highlighting via a custom `Highlighter`
- right prompt via `WinuxshPrompt::render_prompt_right`
- vi/emacs prompt indicators via `render_prompt_indicator`

Difficulty:

- history-only autosuggestions: **easy**
- config-driven autosuggestion style/min chars: **easy**
- command/path syntax highlighting: **moderate**
- parser-aware highlighting using rubash tokenizer/parser: **moderate to hard**
- ZLE widget compatibility: **defer**

### Prompt / Theme

Files:

- `crates/winuxsh-runtime/src/prompt.rs`
- `crates/winuxsh-runtime/src/theme.rs`

Current prompt already translates:

- `%#`
- `%n`
- `%m`
- `%~`

Feasible next prompt compatibility:

- `RPROMPT`
- `%F{color}` / `%f`
- `%B` / `%b`
- simple Oh My Zsh theme variables
- theme-name discovery from `ZSH_THEME`

Hard or deferred:

- `precmd` hooks
- arbitrary theme helper functions
- dynamic prompt functions beyond a small native set such as Git status

Difficulty: **moderate**.

## Proposed Runtime Module

Add:

```text
crates/winuxsh-runtime/src/zsh_compat/
├── mod.rs
├── config.rs
├── scanner.rs
├── parser.rs
├── omz.rs
├── report.rs
├── apply.rs
└── completion.rs
```

Initial public API:

```rust
pub struct ZshImportOptions {
    pub enabled: bool,
    pub zdotdir: PathBuf,
    pub import_zshrc: bool,
    pub import_oh_my_zsh: bool,
    pub plugins: Vec<String>,
    pub compat_level: ZshCompatLevel,
}

pub struct ZshImportReport {
    pub source_files: Vec<PathBuf>,
    pub aliases: Vec<ImportedAlias>,
    pub env: Vec<ImportedEnv>,
    pub path_entries: Vec<PathBuf>,
    pub fpath_entries: Vec<PathBuf>,
    pub plugins: Vec<ImportedPlugin>,
    pub theme: Option<String>,
    pub edit_mode: Option<EditorMode>,
    pub zstyles: Vec<ImportedZstyle>,
    pub completion_assets: Vec<CompletionAsset>,
    pub diagnostics: Vec<ZshCompatDiagnostic>,
}

pub fn scan(options: &ZshImportOptions) -> ZshImportReport;
pub fn apply_safe_env(report: &ZshImportReport);
pub fn apply_safe_aliases(report: &ZshImportReport, executor: &mut rubash::executor::Executor);
```

`scan()` should be pure and testable. `apply_*()` should be small and explicit.

## Safe First Parser Scope

Support first:

- `export KEY=value`
- `KEY=value`
- `alias name='value'`
- `plugins=(git npm docker)`
- `ZSH_THEME="robbyrussell"`
- `ZSH="$HOME/.oh-my-zsh"`
- `ZSH_CUSTOM="$ZSH/custom"`
- `fpath=(... $fpath)`
- `path=(... $path)`
- `bindkey -e`
- `bindkey -v`
- `zstyle '<context>' <key> <value...>`
- `source $ZSH/oh-my-zsh.sh` as a signal only

Explicitly report and skip:

- `zle -N`
- `bindkey ... custom-widget`
- `zmodload`
- `zpty`
- `autoload` except as a completion signal
- `source` of arbitrary files
- global aliases (`alias -g`)
- suffix aliases (`alias -s`)
- shell functions except optional later bash-compatible import
- command substitutions in env/path values unless they are whitelisted later

## Windows PATH Handling

Zsh-style config commonly writes:

```zsh
export PATH="$HOME/bin:$HOME/.local/bin:$PATH"
```

On native Windows, `PATH` must remain semicolon-separated for rubash command
lookup and winuxcmd PATH injection. Therefore, the scanner should not directly
copy zsh PATH strings into process env. It should extract path entries, expand
safe variables (`$HOME`, `~`, `$ZSH`, `$ZSH_CUSTOM`), normalize them as Windows
paths, then join them using `;`.

`path=(...)` / `fpath=(...)` should be treated as structured records, not raw
shell assignments.

## Feasibility Matrix

| Area | Feasibility | Difficulty | Recommended phase |
| --- | --- | --- | --- |
| `[zsh]` config parsing | High | Easy | Phase 1 |
| CLI compatibility report | High | Easy | Phase 1 |
| `.zshrc` scanner for aliases/env/plugins/theme | High | Easy to moderate | Phase 1 |
| Safe PATH/path/fpath extraction | High | Moderate | Phase 1 |
| Oh My Zsh plugin directory discovery | High | Moderate | Phase 2 |
| Alias-only plugin import | High | Moderate | Phase 2 |
| `_cmd` / `#compdef` discovery | High | Easy | Phase 2 |
| Simple `compdef _foo bar=baz` mapping | High | Easy | Phase 2 |
| `_arguments` parser | Medium | Hard | Phase 3 |
| Dynamic zsh completion functions | Low | Very hard | Defer |
| History autosuggestions | High | Easy | Phase 4 |
| Completion-based autosuggestions | Medium | Moderate to hard | Later |
| Syntax highlighting | High | Moderate | Phase 5 |
| Parser-aware syntax highlighting | Medium | Moderate to hard | Phase 5+ |
| RPROMPT / prompt indicators | High | Easy to moderate | Phase 6 |
| Oh My Zsh theme translation | Medium | Moderate | Phase 6 |
| ZLE widget compatibility | Low | Very hard | Defer |
| `zmodload` / `zpty` compatibility | Low | Very hard | Defer |

## Implementation Recommendation

Start with a **report-only scanner**. It gives users and agents visibility into
what can be imported without risking startup behavior.

First code milestone:

- add `zsh_compat` module with pure scanner/report types
- add fixtures under `crates/winuxsh-runtime/tests/fixtures/zsh_compat/`
- add unit/integration tests for:
  - Oh My Zsh template
  - aliases and exports
  - plugin arrays
  - `ZSH_THEME`
  - `bindkey -e/-v`
  - `zstyle`
  - unsupported ZLE/zmodload diagnostics
- add `winuxsh --zsh-compat-report`
- do not auto-apply imports yet

Second milestone:

- apply safe env/PATH before `Executor::new()`
- apply imported aliases after executor construction
- add completion asset discovery to completion dirs/report
- keep non-interactive mode quiet unless a compatibility command is explicitly
  requested

This is the lowest-risk path that still moves directly toward practical
`.zshrc` and Oh My Zsh compatibility.
