---
tags: [winuxsh, zsh, plugins, native-packs, roadmap]
created: 2026-07-19
status: active
---

# Winuxsh Native Zsh Plugin Pack Plan

> Goal: make winuxsh feel like a useful zsh-style Windows-native shell out of
> the box, without vendoring or sourcing Oh My Zsh / zsh plugin scripts.

## Product Rule

Preinstalled zsh plugin support means **native winuxsh packs**, not bundled zsh
source code:

- no vendored Oh My Zsh / zsh plugin source in this repo
- no normal-startup `source plugin.zsh`
- no zsh parser, no ZLE runtime, no `zmodload`, no `zpty`
- shell syntax and execution stay in rubash
- interactive UX stays in reedline + native winuxsh modules
- `.zshrc` remains a familiar import source, while `~/.winshrc.toml` remains
  the deterministic native control plane

## Current Native Pack Inventory

Already implemented:

| Area | Current support | Default behavior |
| --- | --- | --- |
| `git` | native Oh My Zsh-style alias pack (`g`, `gst`, `gco`, etc.) | available through zsh import/plugin report |
| `docker` | native alias pack | available through zsh import/plugin report |
| `kubectl` | native alias pack + disabled dynamic completion preset | available through zsh import/plugin report |
| `npm` | native alias pack + runtime completion shape detection | available through zsh import/plugin report |
| `zsh-autosuggestions` | native reedline history hinter | enabled by default as native UX |
| `zsh-syntax-highlighting` | native reedline highlighter, `main` subset | enabled by default as native UX |
| `zsh-history-substring-search` | native widget preset mapped to reedline history navigation | available when `[zsh.native_widgets]` is enabled |
| standard ZLE widgets | common `bindkey KEY widget` mapped to reedline events | import-plan only until `[zsh.native_widgets]` is enabled |
| `direnv` | native lifecycle hook shim | explicit opt-in |
| `dotenv` | safe `.env` parser on `precmd` / `chpwd` | explicit opt-in |
| `zoxide` | native `z` command shim + directory tracking | explicit opt-in |
| `thefuck` | native `fuck` correction shim | explicit opt-in |
| `command-not-found` | Windows-native install/search hints | explicit opt-in |
| `fzf` / `zsh-interactive-cd` | native `cdf` / `fzf-cd` directory selector shim | explicit opt-in |
| `last-working-dir` | native cache + `lwd` command + optional REPL restore | explicit opt-in |

The main gap is not raw plugin compatibility. The gap is a **pack manifest and
profile layer** that tells users/agents what is built in, what is recommended,
what is enabled by default, and what requires explicit trust.

## Recommended Preinstalled Set

Ship these as native winuxsh packs that are present in the binary and visible in
the pack inventory, but do not vendor zsh plugin source:

| Pack | Purpose | Startup default | Recommended profile |
| --- | --- | --- | --- |
| `zsh-autosuggestions` | history-based inline suggestions | on | `agent`, `zsh-lite`, `full` |
| `zsh-syntax-highlighting` | command/word highlighting | on | `agent`, `zsh-lite`, `full` |
| `zsh-history-substring-search` | zsh-like history navigation widget preset | off | `zsh-lite`, `full` |
| standard ZLE widgets | common `bindkey` mappings to reedline events | off | `zsh-lite`, `full` |
| `git` | daily Oh My Zsh-style aliases and prompt/completion polish | off | `zsh-lite`, `full` |
| `docker`, `kubectl`, `npm` | common developer tool aliases/completion hints | off | `full` when binaries exist |
| `command-not-found` | interactive missing-command hints | off | optional in `zsh-lite`, on in `full` |
| `zoxide`, `fzf`, `direnv`, `dotenv`, `thefuck`, `last-working-dir` | lifecycle/state/external-command UX | off | explicit opt-in / `full` review |

Default-on remains deliberately small: only native UI helpers that do not run
external commands and do not mutate user state. Everything else is preinstalled
as an available pack, but activated by a profile plan or explicit config.

## Preinstall Tiers

### Tier A - Safe Always-On UX

Safe to enable by default because it does not execute external commands and
does not mutate user state:

- history autosuggestions from reedline history
- main syntax highlighting
- Tab completion menu and Ctrl+R history menu
- prompt indicators

Current status: mostly implemented. Keep it enabled unless users explicitly
disable it.

### Tier B - Recommended Zsh-Lite Profile

Useful for users who want the familiar Oh My Zsh feel, but should be enabled
through an explicit profile/import command rather than silently mutating every
shell:

- `git` alias pack
- `zsh-autosuggestions` widget preset (`Ctrl+Space` accept)
- `zsh-history-substring-search` widget preset
- standard ZLE bindkey import
- optional `command-not-found` for interactive-only hints

This profile should be the first "preinstalled plugin pack" user-facing
surface.

### Tier C - Tool-Specific Recommended Packs

Useful when the matching external command exists, but not safe as a universal
default:

- `docker`
- `kubectl`
- `npm`
- future: `gh`, `cargo`, `rustup`, `pnpm`, `python`, `pip`, `uv`

Behavior:

- static aliases can be proposed in a plan
- dynamic completions remain disabled unless allowlisted
- missing external binaries are reported, not treated as startup failures

### Tier D - Explicit Trust / Lifecycle Packs

These execute external commands, read project files, or change cwd/state, so
they must remain opt-in:

- `direnv`
- `dotenv`
- `zoxide`
- `thefuck`
- `fzf`
- `zsh-interactive-cd`
- `last-working-dir`

Behavior:

- never enabled only because `.zshrc` mentions the plugin
- import-plan may suggest disabled config
- doctor/status commands should explain required binaries and trust boundary

## Proposed Config Surface

Do not add a new config model until necessary. The first implementation can
generate existing config sections:

```toml
[zsh]
enabled = true
auto_apply = true
plugins = ["git", "zsh-autosuggestions", "zsh-history-substring-search"]

[zsh.native_widgets]
enabled = true
presets = ["autosuggestions", "history_substring_search"]
import_bindkeys = true

[zsh.native_plugins]
enabled = true
presets = ["command-not-found"]
```

Later, if the UX needs a single knob, add a small profile field:

```toml
[zsh.profile]
name = "zsh-lite" # none | zsh-lite | agent | full
```

But the first milestone should avoid adding unnecessary config surface.

## CLI Surface

Add read-only inventory first:

```text
winuxsh --zsh-native-packs
winuxsh --zsh-native-packs-json
```

Then add profile planning:

```text
winuxsh --zsh-profile-plan zsh-lite
winuxsh --zsh-profile-plan agent
```

Only after the plan output is stable:

```text
winuxsh --zsh-profile-apply zsh-lite
```

Rules:

- `plan` prints a reviewable TOML patch.
- `apply` uses the same backup + managed-block mechanics as
  `--zsh-compat-import-apply`.
- no command should vendor, download, or source zsh plugin scripts.

## Profiles

### `agent`

Goal: deterministic non-interactive defaults and no alias surprise.

- autosuggestions: irrelevant to non-interactive, harmless for REPL
- syntax highlighting: on
- native widgets: off unless user asks
- aliases: none
- native plugins: none
- dynamic/runtime completions: off

### `zsh-lite`

Goal: familiar zsh/Oh My Zsh daily UX with low risk.

- plugins: `git`, `zsh-autosuggestions`, `zsh-history-substring-search`
- native widgets: `autosuggestions`, `history_substring_search`
- syntax highlighting: on
- command-not-found: optional interactive hint, off in the first profile plan if
  we want zero extra output
- dynamic/runtime completions: off

### `full`

Goal: opt-in power-user setup.

- everything in `zsh-lite`
- tool packs for installed tools: `docker`, `kubectl`, `npm`, future `gh`,
  `cargo`, `pnpm`
- optional lifecycle packs: `zoxide`, `fzf`, `dotenv`, `direnv`,
  `last-working-dir`, `thefuck`
- dynamic completions only for explicitly available and allowlisted commands

## Implementation Phases

### Phase 31 - Native Pack Manifest

Implementation status: completed on master. Winuxsh now exposes a typed
`NativeZshPack` registry through `winuxsh --zsh-native-packs` and
`winuxsh --zsh-native-packs-json`. The commands are read-only inventory
surfaces and do not load user config, execute external commands, or change
startup behavior.

Create a typed registry of native packs:

- pack name
- kind: alias, widget, highlighter, completion, lifecycle, command shim
- risk tier: always-on, profile, tool-specific, explicit-trust
- required external binaries
- aliases count
- generated config sections
- report/import-plan support status

Expose it through:

- `winuxsh --zsh-native-packs`
- `winuxsh --zsh-native-packs-json`

Verification:

- unit tests for registry contents
- CLI text and JSON snapshots
- no startup behavior change

### Phase 32 - Zsh-Lite Profile Plan

Implementation status: completed on master. Winuxsh now exposes read-only
`winuxsh --zsh-profile-plan <profile>` for `agent` and `zsh-lite`. It generates
a reviewable TOML patch only; it does not write `~/.winshrc.toml` or enable
lifecycle/external-command packs automatically.

Add a profile planner that generates a managed TOML block for `zsh-lite` using
existing config fields.

Scope:

- enable safe zsh compatibility
- include `git`, `zsh-autosuggestions`, `zsh-history-substring-search`
- enable native widgets and bindkey import
- keep dynamic completions and lifecycle plugins disabled

Verification:

- profile plan snapshot tests
- apply/status/rollback mechanics reuse existing import block code
- existing compat and zsh_compat tests remain green

Output rules:

- `agent`: enable safe zsh compatibility and native UI defaults, but keep
  aliases, native widgets, native plugins, dynamic completions, and runtime
  completions disabled for deterministic non-interactive use.
- `zsh-lite`: enable safe zsh compatibility, include `git`,
  `zsh-autosuggestions`, and `zsh-history-substring-search`, enable native
  widgets with `autosuggestions` and `history_substring_search`, import standard
  bindkeys, and keep native plugins / dynamic completions disabled.
- unknown profile names fail before printing a partial plan.

### Phase 33 - Git Daily-Use Polish

Make `git` feel first-class even without Oh My Zsh files:

- keep existing native aliases
- add built-in git completion definition for common subcommands and flags
- expose `{git_prompt}` docs and zsh theme import behavior
- ensure `gst`, `gco`, `glog`, `gp`, `gl` are covered in tests

Verification:

- completion tests for `git <Tab>` and common subcommand flags
- zsh import tests preserve user alias overrides

### Phase 34 - Widget Pack Polish

Close gaps between "mapped" and "useful" widget behavior:

- autosuggest accept / partial accept keybindings
- history substring search behavior beyond plain Up/Down if reedline exposes the
  needed hook
- standard ZLE bindkey coverage audit
- explicit report for unsupported widgets such as `fzf-tab`

Verification:

- reedline keybinding tests
- import-plan tests for common zsh widget plugin declarations

### Phase 35 - Tool Pack Expansion

Add more native packs based on daily Windows developer usage:

- `gh`: aliases + completion provider if available
- `cargo` / `rustup`: aliases + completions
- `pnpm` / `node`: aliases + runtime completions
- `python` / `pip` / `uv`: aliases + completions

Rules:

- static aliases are safe but must not override user aliases
- dynamic completions require allowlist + timeout + cache
- missing binaries are diagnostics only

### Phase 36 - Profile Apply and Doctor Integration

Make the profile layer operational:

- `--zsh-profile-apply <profile>` writes a managed block with backup
- `--zsh-compat-doctor` reports native pack availability and missing binaries
- README documents `agent`, `zsh-lite`, and `full`

Verification:

- backup/rollback tests
- doctor output tests
- README examples match generated config

## Immediate Recommendation

Start with Phase 31 and Phase 32. They give users a concrete "preinstalled zsh
plugin pack" experience without changing startup behavior or taking new
execution risks. After that, build Git polish in Phase 33 because Git is the
highest-value daily shell plugin.
