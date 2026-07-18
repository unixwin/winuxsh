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

Decision: `.zshrc` should become the familiar user-facing compatibility entry
point, while `~/.winshrc.toml` remains the native winuxsh control plane. TOML is
not redundant: it is the deterministic place for safe import/apply state,
Windows-native overrides, agent-readable diagnostics, and rollback-safe managed
blocks. Winuxsh can read and translate `.zshrc`, but it should not execute zsh
startup files as the runtime authority.

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

Current implementation status: scanner and `--zsh-compat-report` /
`--zsh-compat-report-json` CLI are implemented on `codex/zsh-compat-scanner`.
Opt-in startup import is available behind `[zsh].enabled = true` plus
`[zsh].auto_apply = true` for known-safe env/PATH records and aliases only.
Completion assets, theme hints, and editor hints remain report-only until their
native translators are implemented.

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
- opt-in safe application path:
  - env/PATH records apply before `rubash::Executor::new()`
  - `winuxcmd` PATH injection still runs after zsh PATH import so coreutils win
  - imported aliases apply after executor construction through rubash `alias`
    builtin semantics
  - native `.winshrc.toml` aliases apply last and override imported names

Tests:

- fixture `.zshrc` files for Oh My Zsh template, simple aliases, plugin arrays,
  fpath, zstyle, bindkey.
- safe apply tests for PATH de-duplication, env whitelist filtering, and rubash
  alias installation.

## Phase 2 - Oh My Zsh Layout Importer

Implementation status: Phase 2a is implemented on `codex/zsh-compat-scanner`.
Static completion assets already discovered by the scanner are translated into
winuxsh `CommandDef` records. This covers `#compdef`, simple `compdef`, and
simple `_arguments` option specs. It does not execute zsh completion functions
and does not attempt dynamic `compadd`, `_describe`, cache, or ZLE behavior.
Translated definitions merge with built-in winuxcmd defaults, while user TOML
completion definitions still take highest priority.

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

Implementation status: Phase 2b is implemented on `codex/zsh-compat-scanner`.
The import report now includes explicit plugin tier metadata:
`completion_only`, `alias_only`, `alias_and_completion`, `native_ux`,
`partial`, `unsupported`, and `missing`. This keeps startup safe while giving
users and agents a concrete compatibility map for common Oh My Zsh plugins.

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

Implementation status: Phase 4a is implemented on `codex/zsh-compat-scanner`.
The first implementation is intentionally native and narrow: winuxsh now wires
a reedline history hinter into the REPL, honors familiar zsh-autosuggestions
configuration names where they map cleanly, and keeps completion/match-prev-cmd
strategies report-only until tests exist.

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

Current supported subset:

- `history` strategy via native reedline hints.
- `ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE` subset: `fg=`, `bg=`, `bold`,
  `underline`, `italic`, `standout` / `reverse`.
- `ZSH_AUTOSUGGEST_BUFFER_MAX_SIZE`.
- Native TOML override under `[zsh.autosuggestions]`.

## Phase 5 - Native Syntax Highlighting

Implementation status: Phase 5a is implemented on `codex/zsh-compat-scanner`.
Winuxsh now provides a native reedline highlighter for the
zsh-syntax-highlighting `main` highlighter subset. It does not source
`zsh-syntax-highlighting.zsh` and does not depend on ZLE variables such as
`BUFFER` or `region_highlight`.

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

Current supported subset:

- command position known/unknown highlighting
- shell builtins, reserved words, command separators, redirections
- existing paths and path prefixes
- single/double quoted arguments, unquoted variables, command substitutions
- single/double hyphen options, assignments, comments
- `ZSH_HIGHLIGHT_STYLES[key]=...` scan/import for supported style keys
- native TOML override under `[zsh.syntax_highlighting]`

Do not run `zsh-syntax-highlighting.zsh`; it depends on ZLE internals and zsh
parameters like `BUFFER`, `PREBUFFER`, and `region_highlight`.

Later phases can add zsh's non-default highlighters such as `brackets`,
`pattern`, `regexp`, `cursor`, and `line` once the main highlighter is stable.

## Phase 6 - Prompt and Theme Compatibility

Implementation status: Phase 6a is implemented on `codex/zsh-compat-scanner`.
Winuxsh now translates common static zsh prompt assignments into native
winuxsh prompt templates and reports unsupported dynamic segments. It does not
source theme scripts or execute prompt substitutions.

Translate common zsh prompt/theme forms:

- `PROMPT`
- `RPROMPT`
- `%~`, `%n`, `%m`, `%#`
- `%F{color}`, `%f`, `%B`, `%b`
- Oh My Zsh `ZSH_THEME`
- common Git prompt variables

Native output should become winuxsh prompt/theme config, not arbitrary zsh theme
execution.

First-pass scope:

- scan `.zshrc` and simple Oh My Zsh theme files for `PROMPT` / `PS1` and
  `RPROMPT` / `RPS1`.
- translate common prompt escapes into winuxsh placeholders:
  `{user}`, `{host}`, `{cwd}`, `{symbol}`.
- strip or report color/style escapes and unsupported dynamic command
  substitutions.
- let native `[shell].prompt_format` override imported zsh prompts.
- expose translated prompts in `--zsh-compat-report` and
  `--zsh-compat-report-json`.

Current supported subset:

- `PROMPT` / `PS1` and `RPROMPT` / `RPS1` from `.zshrc`.
- static Oh My Zsh theme files under `$ZSH/themes` or `$ZSH_CUSTOM/themes`.
- prompt escapes `%n`, `%m`, `%M`, `%~`, `%/`, `%d`, `%c`, `%C`, `%1~`,
  `%2~`, `%3~`, `%#`, and `%%`.
- color/style escapes `%F{...}`, `%K{...}`, `%f`, `%k`, `%B`, `%b`, `%U`,
  `%u`, `%S`, `%s`, `%{...%}` are stripped safely.
- unsupported prompt substitutions such as `${...}`, backticks, `%D{...}`,
  `git_prompt_status`, `git_prompt_ahead`, and conditional `%(... )` are
  reported as unsupported prompt segments.
- native `[shell].prompt_format` and `[shell].right_prompt_format` remain
  authoritative over imported zsh prompts.

Implementation status: Phase 6b is implemented on
`codex/zsh-compat-scanner`.

Phase 6b adds a native bridge for common Oh My Zsh Git prompt forms:

- translate `$(git_prompt_info)` and escaped `\$(git_prompt_info)` to a native
  `{git_prompt}` placeholder.
- scan `ZSH_THEME_GIT_PROMPT_PREFIX` and `ZSH_THEME_GIT_PROMPT_SUFFIX` from
  `.zshrc` or static theme files, stripping zsh color/style escapes.
- render `{git_prompt}` from native `.git/HEAD` discovery instead of executing
  zsh Git helper functions.
- keep detailed `git_prompt_status`, `git_prompt_ahead`, async Git, dirty, and
  per-file status segments report-only until a tested native status provider
  exists.

## Phase 7 - Oh-My-Winuxsh Compatibility Layer

Implementation status: Phase 7a through Phase 7e are implemented on
`codex/zsh-compat-scanner`.

Phase 7a adds a safe local import-plan command:

- `winuxsh --zsh-compat-import-plan` scans the current zsh setup and prints a
  reviewable `.winshrc.toml` patch.
- The command must not write `~/.winshrc.toml` or copy plugin/theme files.
- The generated patch should enable `[zsh]` safe auto-apply, preserve zsh
  plugin names, and include native prompt/editor/alias translations where they
  are already supported.
- Unsupported features remain visible through `--zsh-compat-report`.

Phase 7b adds an explicit local apply command:

- `winuxsh --zsh-compat-import-apply` writes the same generated import block to
  `~/.winshrc.toml`.
- The command must create a timestamped backup before writing.
- The command may replace only the previous winuxsh-managed zsh import block;
  user-authored TOML outside that block must remain untouched.
- If the generated block would duplicate existing user-authored TOML tables,
  the command must fail before writing and tell users to merge the plan
  manually.
- The command must stay explicit and one-shot. Startup must continue to read
  native TOML only and must not mutate user config.

Phase 7c adds a read-only status command:

- `winuxsh --zsh-compat-import-status` inspects `~/.winshrc.toml` without
  writing it.
- The command reports whether the config exists, whether the winuxsh-managed
  import block is missing/present/malformed, whether current TOML parses, and
  whether a new apply would add, replace, or fail before writing.
- The command reports discovered backup files so users and agents can see
  whether a previous apply created a rollback point.

Phase 7d adds a read-only rollback plan command:

- `winuxsh --zsh-compat-import-rollback-plan` inspects the backup files created
  by `--zsh-compat-import-apply`.
- The command prints the latest rollback source and destination, plus the
  exact PowerShell copy command a user or agent can run.
- The command must not restore files automatically. A future explicit rollback
  apply command can be added after the plan command is tested.

Phase 7e adds a read-only doctor command:

- `winuxsh --zsh-compat-doctor` aggregates the zsh scan report, import status,
  and rollback plan into a compact operator-facing summary.
- The command should answer: what was discovered, whether `apply` is safe, what
  blocks it if not, and whether a rollback point exists.
- The command must remain read-only and must not replace the detailed report,
  JSON report, import-plan, status, or rollback-plan commands.

Once the importer works, build a local package layer:

- install/import zsh-compatible completion packs
- install/import themes
- install native autosuggestion/highlighting modules
- produce import reports for existing Oh My Zsh setups

This should start local-only. Online registry behavior comes later.

## Phase 8 - Native Plugin Packs

Implementation status: Phase 8a is implemented on
`codex/zsh-compat-scanner`.

Phase 8a starts the local-only native plugin pack layer with the Oh My Zsh
`git` plugin:

- If `.zshrc` declares `plugins=(git)` but no readable Oh My Zsh `git`
  plugin directory is available, winuxsh should still provide a conservative
  native alias pack.
- Native aliases must be marked with `origin = "native-plugin:git"` in the
  report and import plan.
- Native aliases must not override aliases already discovered from the user's
  zsh files.
- No zsh plugin scripts are executed and no Oh My Zsh source is vendored.

Later Phase 8 work can add native packs for `docker`, `npm`, `node`, `python`,
`pip`, `kubectl`, and related completion metadata after each pack has tests.

Implementation status: Phase 8b is implemented on
`codex/zsh-compat-scanner`.

Phase 8b adds the Oh My Zsh `docker` plugin as a conservative native alias pack:

- If `.zshrc` declares `plugins=(docker)` but no readable Oh My Zsh `docker`
  plugin directory is available, winuxsh provides static Docker aliases derived
  from the upstream Oh My Zsh plugin.
- Native aliases are marked with `origin = "native-plugin:docker"` in the report
  and import plan.
- Native Docker aliases do not override aliases already discovered from the
  user's zsh files.
- The dynamic Docker completion/cache logic in the Oh My Zsh plugin remains
  report-only future work; winuxsh does not execute that zsh code.

## Non-Goals

- Do not vendor zsh, Nushell, Oh My Zsh, or zsh plugin source into the winuxsh
  repository.
- Do not make `.zshrc` the only runtime config or execute it directly; TOML
  remains the native control plane and rollback-safe import target.
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
