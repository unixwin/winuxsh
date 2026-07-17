---
tags: [winuxsh, zsh, compatibility, roadmap, plugins]
created: 2026-07-17
status: active
---

# Zsh Compatibility Plan

> Goal: make winuxsh feel familiar to zsh users on native Windows, while keeping
> execution semantics in rubash and avoiding MSYS2/Git Bash/WSL isolation.

## Compatibility Policy

Winuxsh should aim for zsh compatibility in this order:

1. **Interactive UX compatibility**: keybindings, completions, prompt behavior,
   autosuggestions, syntax highlighting, history search.
2. **Profile compatibility**: import common `.zshrc` settings safely.
3. **Plugin compatibility**: support Oh My Zsh-style plugin discovery and
   translate/import useful plugin assets.
4. **Semantic compatibility**: do not implement zsh shell syntax in winuxsh;
   shell semantics remain rubash/bash-like.

This means `.zshrc` compatibility should be opt-in and diagnostic. Winuxsh
should not blindly source arbitrary zsh scripts as if it were zsh.

## Direct Compatibility Boundary

The goal is for a zsh user to point winuxsh at an existing zsh setup and get a
useful native Windows shell without rewriting everything by hand. The supported
model should be:

- **Read existing zsh files**: discover `.zshenv`, `.zprofile`, `.zshrc`,
  Oh My Zsh, `$ZSH_CUSTOM`, themes, and plugin directories.
- **Translate safe settings**: aliases, exports, PATH/path/fpath entries,
  plugin names, theme names, simple prompt escapes, completion assets, and
  editor mode.
- **Implement editor UX natively**: autosuggestions, syntax highlighting,
  history search, prompt indicators, completion menus.
- **Report unsupported constructs**: arbitrary ZLE widgets, `zmodload`, `zpty`,
  dynamic completion scripts, and plugins that require a real zsh interpreter.

This gives practical zsh compatibility while preserving the product contract:
Windows-native process behavior, bash-like execution through rubash, and no
MSYS2/Git Bash/WSL isolation.

## Proposed Config Surface

Keep `~/.winshrc.toml` as the native authoritative config, then add a zsh
compatibility section:

```toml
[zsh]
enabled = true
zdotdir = "~"
import_zshrc = true
import_oh_my_zsh = true
plugins = ["git", "zsh-autosuggestions", "zsh-syntax-highlighting"]
compat_level = "safe" # safe | warn | experimental
```

Compatibility modes:

- `safe`: scan and translate known-safe records only; never execute sourced
  zsh files or plugin scripts.
- `warn`: import safe records and emit diagnostics for unsupported lines.
- `experimental`: allow additional translators for simple functions/prompts
  after tests exist; still do not source arbitrary zsh scripts at startup.

Also support familiar zsh environment variables where practical:

- `ZDOTDIR`
- `ZSH`
- `ZSH_CUSTOM`
- `ZSH_THEME`
- `CASE_SENSITIVE`
- `HYPHEN_INSENSITIVE`
- `ZSH_AUTOSUGGEST_*`
- `ZSH_HIGHLIGHT_STYLES`

## Phase 1 - Zsh Profile Scanner

Implementation interface audit: `DOCS/zsh-compatibility-interface-audit.md`.

Current implementation status: report-only scanner and `--zsh-compat-report` / `--zsh-compat-report-json` CLI are implemented on `codex/zsh-compat-scanner`; automatic startup import remains disabled.

Build a scanner/parser for zsh profile files. It should read but not execute:

- `${ZDOTDIR:-$HOME}/.zshrc`
- optionally `.zshenv` only for safe env assignments
- Oh My Zsh template patterns

Supported first:

- `export KEY=value`
- `alias name='value'`
- simple `PATH=...` / `path=(...)`
- `fpath=(...)`
- `plugins=(...)`
- `ZSH_THEME="..."`
- `source $ZSH/oh-my-zsh.sh` as a signal, not a direct source operation
- `bindkey -e` / `bindkey -v`
- simple `zstyle '<context>' <key> <value...>`

Output:

- an import report
- native config suggestions
- imported aliases/completion dirs/theme settings where safe
- a stable diagnostic format suitable for agents and CI snapshots

Tests:

- fixture `.zshrc` files for Oh My Zsh template, simple aliases, plugin arrays,
  fpath, zstyle, bindkey.

## Phase 2 - Oh My Zsh Layout Importer

Support Oh My Zsh-like discovery:

- `$ZSH/plugins/<plugin>/<plugin>.plugin.zsh`
- `$ZSH/plugins/<plugin>/_<plugin>`
- `$ZSH_CUSTOM/plugins/<plugin>/...`
- `$ZSH/themes/<theme>.zsh-theme`
- `$ZSH_CUSTOM/themes/<theme>.zsh-theme`

Import behavior:

- `_cmd` completion files become candidate zsh completion assets.
- simple alias-only plugin snippets become aliases.
- `compdef` mappings become command completion metadata.
- unsupported ZLE/zmodload/zpty lines are reported and skipped.

Do not execute arbitrary plugin scripts during startup.

Plugin compatibility should be tiered:

| Tier | Plugin shape | Initial behavior |
| --- | --- | --- |
| 1 | completion-only `_cmd` files and `#compdef` | import/translate |
| 1 | alias-only plugins | import aliases |
| 2 | simple prompt/theme snippets | translate common prompt escapes |
| 2 | simple shell functions compatible with rubash/bash | optional later translator |
| 3 | zsh-autosuggestions / zsh-syntax-highlighting | native reedline/rubash implementation |
| 4 | ZLE widgets, `zmodload`, `zpty`, deep zsh internals | report and skip |

## Phase 3 - Zsh Completion Compatibility

Add an importer for zsh completion files:

- parse `#compdef` headers
- parse common `_arguments` forms where practical
- parse simple `compdef _foo foo` mappings
- preserve user override order
- emit/derive winuxsh TOML completion definitions

Fallback:

- if a zsh completion is too dynamic, keep it as unsupported and point users to
  winuxcmd/help-derived or TOML completion definitions.

## Phase 4 - Native Autosuggestions

Implement zsh-autosuggestions behavior natively in reedline:

- history strategy first
- optional completion strategy later
- muted right-of-cursor suggestion
- accept full suggestion with common forward/end widgets
- partial accept with word-forward actions if reedline exposes enough hooks

Honor familiar config names when set:

- `ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE`
- `ZSH_AUTOSUGGEST_STRATEGY`
- `ZSH_AUTOSUGGEST_BUFFER_MAX_SIZE`
- selected widget lists if they can map to reedline events

## Phase 5 - Native Syntax Highlighting

Implement zsh-syntax-highlighting-like behavior natively:

- command position known/unknown
- paths and path prefixes
- strings and quotes
- variables and command substitutions
- redirections and pipes
- comments
- errors/incomplete syntax where rubash tokenizer/parser can expose them

Honor a subset of:

- `ZSH_HIGHLIGHT_STYLES`
- `ZSH_HIGHLIGHT_HIGHLIGHTERS`

Do not run `zsh-syntax-highlighting.zsh`; it depends on ZLE internals and zsh
parameters like `BUFFER`, `PREBUFFER`, and `region_highlight`.

## Phase 6 - Prompt and Theme Compatibility

Translate common zsh prompt/theme forms:

- `PROMPT`
- `RPROMPT`
- `%~`, `%n`, `%m`, `%#`
- `%F{color}`, `%f`, `%B`, `%b`
- Oh My Zsh `ZSH_THEME`
- common Git prompt variables

Native output should become winuxsh prompt/theme config, not arbitrary zsh theme
execution.

## Phase 7 - Oh-My-Winuxsh Compatibility Layer

Once the importer works, build a local package layer:

- install/import zsh-compatible completion packs
- install/import themes
- install native autosuggestion/highlighting modules
- produce import reports for existing Oh My Zsh setups

This should start local-only. Online registry behavior comes later.

## Non-Goals

- Do not vendor zsh, Nushell, Oh My Zsh, or zsh plugin source into the winuxsh
  repository.
- Do not make `.zshrc` the authoritative runtime config before the native TOML
  config is stable.
- Do not execute zsh plugin scripts during normal startup.
- Do not add a zsh parser/executor in winuxsh.
- Do not change `~` away from the normal Windows user home.

## Definition of Done

- A user with a simple Oh My Zsh `.zshrc` can run a winuxsh import command and
  get aliases, plugin completions, theme choice, edit mode, and zsh-like
  autosuggestions/highlighting without breaking startup.
- Unsupported zsh features are reported clearly.
- Non-interactive agent mode remains deterministic and quiet.
- No zsh/Nushell/Oh My Zsh source is vendored into the winuxsh repo.
