---
tags: [winuxsh, zsh, migration, tutorial]
created: 2026-07-19
status: active
---

# Zsh Migration Guide for Winuxsh

This guide is for users who already know zsh / Oh My Zsh and want a similar
experience in winuxsh on native Windows.

Winuxsh is not a zsh interpreter. The goal is to preserve the daily zsh feel
where it matters while keeping Windows-native process behavior and rubash-owned
shell semantics.

## Mental Model

Think of winuxsh as three layers:

```text
rubash       -> bash-compatible shell language and execution
winuxcmd     -> Unix coreutils exposed through Windows PATH
reedline     -> zsh-like interactive frontend
```

Zsh compatibility lives above those layers:

- `.zshrc` is a source of intent, not code that is blindly executed.
- Oh My Zsh plugins are scanned, classified, and translated when safe.
- Common UX plugins are implemented natively in winuxsh.
- Unsupported zsh internals are reported instead of silently ignored.

## What Carries Over Cleanly

These zsh habits should feel familiar:

| Zsh habit | Winuxsh behavior |
| --- | --- |
| `alias gst='git status'` | Imported into native `[aliases]` when safe |
| `export KEY=value` | Imported as safe env where allowed |
| `PATH=...` / `path=(...)` | Imported with Windows-native de-duplication |
| `plugins=(git npm ...)` | Scanned and mapped to native packs or diagnostics |
| `bindkey -e` / `bindkey -v` | Mapped to Emacs / Vi editor mode |
| common `bindkey KEY widget` | Mapped to reedline events for supported standard widgets |
| `PROMPT`, `RPROMPT`, `%~`, `%n`, `%m`, `%#` | Translated to native prompt templates where possible |
| `$(git_prompt_info)` | Translated to native `{git_prompt}` |
| `_cmd` / `#compdef` / simple `_arguments` completions | Translated to native completion definitions when static enough |

## What Does Not Carry Over Directly

These require native replacements or remain unsupported for now:

- arbitrary zsh functions executed during startup
- arbitrary `source plugin.zsh` at shell startup
- ZLE scripting internals such as `BUFFER`, `PREBUFFER`, `region_highlight`
- `zmodload`, `zpty`, deep completion internals like dynamic `compadd`
- zsh-only shell syntax that rubash/bash does not support
- plugins that depend on a running zsh interpreter rather than aliases,
  completion metadata, prompt text, or well-known lifecycle hooks

This is deliberate. Winuxsh must stay Windows-native, agent-friendly, and
rubash-owned for shell semantics.

## Step 1: Inspect Your Zsh Setup

Run a read-only report first:

```pwsh
winuxsh --zsh-compat-report
```

For tools or agents, use JSON:

```pwsh
winuxsh --zsh-compat-report-json
```

The report shows:

- discovered source files
- aliases
- env and PATH entries
- plugin names and tiers
- completion assets
- dynamic completion sources
- native hook/widget suggestions
- prompt/theme translations
- unsupported features with diagnostics

## Step 2: Review the Import Plan

Generate a TOML patch without writing anything:

```pwsh
winuxsh --zsh-compat-import-plan
```

The plan targets `~/.winshrc.toml`. It may include:

```toml
[zsh]
enabled = true
auto_apply = true
plugins = ["git", "zsh-autosuggestions", "zsh-history-substring-search"]

[editor]
edit_mode = "vi"

[aliases]
gst = "git status"

[shell]
prompt_format = "{user}@{host} {cwd} {git_prompt} {symbol}"
```

Do not apply a plan you do not understand. Unsupported zsh behavior should stay
visible in the report.

## Step 3: Apply Explicitly

Apply only after review:

```pwsh
winuxsh --zsh-compat-import-apply
```

Winuxsh creates a backup before writing and only replaces its managed import
block. User-authored TOML outside that block is preserved.

Check the result:

```pwsh
winuxsh --zsh-compat-import-status
winuxsh --zsh-compat-doctor
```

If you need to inspect rollback instructions:

```pwsh
winuxsh --zsh-compat-import-rollback-plan
```

## Step 4: Inspect Native Zsh Packs

Winuxsh preinstalls native equivalents for common zsh plugin behavior. List the
current inventory:

```pwsh
winuxsh --zsh-native-packs
```

Machine-readable version:

```pwsh
winuxsh --zsh-native-packs-json
```

Important distinction:

- **preinstalled** means winuxsh knows how to provide a native implementation.
- **enabled by default** is intentionally much smaller.

Default-on safe UI packs:

- `zsh-autosuggestions`
- `zsh-syntax-highlighting`

Recommended low-risk daily profile, planned as `zsh-lite`:

- `git`
- `zsh-autosuggestions`
- `zsh-history-substring-search`
- standard ZLE widget mappings

Explicit-trust packs stay opt-in:

- `direnv`
- `dotenv`
- `zoxide`
- `thefuck`
- `command-not-found`
- `fzf`
- `zsh-interactive-cd`
- `last-working-dir`

## Step 5: Configure Daily Zsh-Like UX

Until the planned `--zsh-profile-plan zsh-lite` command exists, configure the
same pieces through TOML.

Example starting point:

```toml
[zsh]
enabled = true
auto_apply = true
plugins = ["git", "zsh-autosuggestions", "zsh-history-substring-search"]
compat_level = "safe"

[zsh.native_widgets]
enabled = true
presets = ["autosuggestions", "history_substring_search"]
import_bindkeys = true

[zsh.native_plugins]
enabled = false
presets = []

[editor]
edit_mode = "vi"

[shell]
prompt_format = "{user}@{host} {cwd} {git_prompt} {symbol}"
multiline_indicator = "> "
history_search_indicator = "history: "
history_search_fail_indicator = "history: no match "

[history]
path = "~/.winuxsh_history"
max_size = 10000
ignore_space_prefixed = true

[completions]
matching = "prefix"
case_sensitive = false
max_command_results = 500

[menus]
completion_page_size = 10
history_page_size = 10
max_entry_lines = 5
```

Keep lifecycle packs disabled until you decide what should run in each project.

## Git Plugin Experience

The native Git pack provides common Oh My Zsh-style aliases such as:

| Alias | Command |
| --- | --- |
| `g` | `git` |
| `gst` | `git status` |
| `gco` | `git checkout` |
| `gsw` | `git switch` |
| `gl` | `git pull` |
| `gp` | `git push` |
| `glog` | `git log --oneline --decorate --graph` |
| `grb` | `git rebase` |
| `gsta` | `git stash push` |

User aliases win over native aliases. If your `.zshrc` already defines `gst`,
winuxsh preserves your version.

Prompt support includes native `{git_prompt}` rendering for common Oh My Zsh
`git_prompt_info` patterns.

## Windows Path Rules

Winuxsh is Windows-native, so prefer:

```bash
cd C:/Users/you/repo
ls C:/Users/you
```

Also accepted:

```bash
cd C:\Users\you\repo
ls /c/Users/you
```

But default output should stay Windows-native:

```bash
pwd
# C:/Users/you/repo
```

If a command receives `/c/...`, winuxsh treats it as compatibility input and
normalizes it before invoking native Windows tools where needed.

## Agent Usage

Agents should prefer deterministic non-interactive entry points:

```pwsh
winuxsh -c "pwd; ls; cargo test"
winuxsh script.sh
```

Guidelines:

- Do not rely on interactive plugin prompts in `-c` or script mode.
- Keep lifecycle plugins opt-in and project-aware.
- Use `--zsh-compat-report-json` and `--zsh-native-packs-json` for diagnostics.
- Prefer `C:/...` paths in generated commands.

## Troubleshooting

### The import plan is empty

Check whether winuxsh is scanning the right zsh directory:

```toml
[zsh]
zdotdir = "~"
import_zshrc = true
import_oh_my_zsh = true
```

Then run:

```pwsh
winuxsh --zsh-compat-doctor
```

### A plugin is reported unsupported

That usually means it needs zsh internals. Look for a native pack first:

```pwsh
winuxsh --zsh-native-packs
```

If no pack exists, the plugin may need a future native implementation rather
than direct zsh sourcing.

### A dynamic completion does not run

Dynamic completions are disabled by default. They need explicit allowlists,
timeouts, and cache settings because they execute external commands.

### A path looks like `/c/Users`

That should only be compatibility input. Default visible cwd output should be
`C:/Users/...`. If prompt, `pwd`, and native child process cwd disagree, that is
a host contract bug.

## Roadmap

Near-term zsh onboarding work:

1. Native pack manifest and CLI inventory: implemented.
2. `zsh-lite` profile planner: planned.
3. Git daily-use polish: planned.
4. Widget pack polish: planned.
5. Tool pack expansion for `gh`, `cargo`, `pnpm`, `python`, and related CLIs:
   planned.
6. README and tutorial expansion: active.

## Golden Rule

Do not make winuxsh safer or more compatible by pretending to be zsh. Make it
useful by translating zsh intent into tested Windows-native behavior.