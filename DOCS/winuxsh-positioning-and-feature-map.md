---
tags: [winuxsh, positioning, roadmap, zsh, nushell, v2.3, v3]
created: 2026-07-17
status: active
---

# Winuxsh Positioning and Feature Map

> 目标：以 zsh/Oh My Zsh 作为交互体验、配置兼容和插件生态的主参考；
> Nushell 只作为现代配置、菜单和补全架构的次级参考。
> Winuxsh 的定位是 Windows 原生的 agent/user terminal：
> `rubash` 提供 bash 语义，`winuxcmd.exe` 提供 coreutils，`reedline` 提供 zsh-like REPL/frontend。

## Product Positioning

Winuxsh should be a **Windows-native bash/zsh-style terminal for humans and
agents**, not an MSYS2/Git Bash/WSL-like compatibility environment and not a
new shell language.

- Target user: 人类用户和 coding agents 都需要一个 Windows 原生终端，但希望使用 bash/zsh 语法和 Unix coreutils，而不是 PowerShell 语义。
- Core promise: Windows-native process/env/cwd/home behavior + bash-compatible execution model + winuxcmd coreutils + modern zsh-like interactive UX.
- Compatibility anchor: scripts and command semantics follow rubash; rubash owns parser, executor, builtins, and job-related shell semantics.
- Native anchor: no isolation layer; `~` maps to the normal Windows user home used by PowerShell (`USERPROFILE` / `dirs::home_dir()`), PATH/env/cwd are the real Windows process environment.
- UX anchor: line editor, completions, menus, themes, history search, prompt polish, profile import, and package ergonomics should primarily reference zsh/Oh My Zsh.
- Secondary reference: Nushell remains useful for modern config shape, menu organization, and reedline integration, not for semantics.
- Distribution anchor: winuxcmd is discovered through PATH injection; no DLL/FFI boundary.

## What Zsh Should Define

zsh is the primary compatibility target for interactive shell feel:

- Startup/profile shape: `ZDOTDIR`, `.zshenv`, `.zprofile`, `.zshrc`, `.zlogin`, `.zlogout`.
- Interactive config: aliases, exports, `PATH/path`, `fpath`, `bindkey -e/-v`, `zstyle`, `PROMPT`, `RPROMPT`.
- Completion ecosystem: `_cmd` completion files, `#compdef`, `compdef`, `compinit`, and Oh My Zsh plugin completion assets.
- Plugin ergonomics: `plugins=(git npm ...)`, `$ZSH`, `$ZSH_CUSTOM`, theme files, alias-only plugins, completion-only plugins.
- Native UX expectations: autosuggestions, syntax highlighting, rich history search, mode indicators, right prompt, and fast completion menus.

Winuxsh should be zsh-compatible at the profile/plugin intent layer, not a
zsh interpreter. It should scan and translate useful `.zshrc` and Oh My Zsh
assets, while implementing editor features natively in reedline/rubash.

## What Nushell Still Teaches Us

Nushell is still a useful secondary reference for shell frontend and
configuration surface:

- History is a first-class subsystem: file format, max size, sync behavior, path, isolation, ignore-space-prefixed commands.
- Reedline UX is explicitly configurable: edit mode, cursor shape, buffer editor, keybindings, menus, hints.
- Completion behavior is configurable: prefix/substring/fuzzy algorithms, sort mode, case sensitivity, quick/partial completion, LS_COLORS, external completer bridge.
- Prompt UX is broader than left prompt: right prompt, mode indicators, multiline indicator, transient prompts.
- Plugins have lifecycle and config: plugin directories, plugin-specific config, plugin garbage collection.
- PATH is treated as structured user configuration, but execution remains platform-aware.

These are useful product-design references. They are not a reason to adopt
Nushell's syntax, data pipeline model, plugin ABI, or command semantics.

## What Winuxsh Must Not Become

- Not PowerShell compatibility mode.
- Not MSYS2, Git Bash, Cygwin, or WSL-style filesystem/process isolation.
- Not Nushell structured pipelines.
- Not a data-frame shell.
- Not a zsh interpreter that blindly sources arbitrary zsh plugin scripts.
- Not a reimplementation of rubash parser/executor.
- Not a duplicate implementation of rubash-owned builtins/job-control semantics.
- Not a winuxcmd FFI/DLL host.
- Not a large config/keybinding DSL before basic shell UX is stable.

## Feature Baseline

### Must Stay Solid

- Non-interactive execution:
  - `winuxsh -c "..."`
  - `winuxsh script.sh`
  - full-script AST execution for heredoc, continuation, multi-line `if`/`for`
- Bash-like runtime semantics through rubash:
  - variable expansion
  - command substitution
  - pipelines
  - aliases/functions
  - shell control flow
  - rubash-owned builtins and job-related semantics
- Windows Unix tool layer:
  - `winuxcmd.exe` PATH injection
  - configured `[winuxcmd].path`
  - no PowerShell wildcard or alias behavior in command execution
  - no isolated filesystem root; `~` is the Windows user home
- Agent/user terminal contract:
  - deterministic `-c` and script-mode behavior
  - faithful stdout/stderr/exit code propagation
  - no interactive banner/noise in non-interactive mode
  - native Windows environment inheritance
- REPL basics:
  - persistent history at `~/.winuxsh_history`
  - Ctrl+C handling
  - Emacs and Vi edit modes
  - Ctrl+R history search
- Completion basics:
  - command/path/env-var completion
  - TOML completion definitions
  - bash completion import
  - built-in winuxcmd definitions
  - user completion dirs override built-ins
- Theme/prompt basics:
  - prompt template
  - built-in themes
  - `~/.winuxsh/themes/*.toml`
- Engineering baseline:
  - CI on master
  - `cargo fmt --check`
  - locked builds
  - compat fixtures

## Feature Map Against Zsh-Like Shells

| Area | zsh / modern-shell capability | Winuxsh status | Direction |
| --- | --- | --- | --- |
| Shell language | zsh syntax plus bash-like scripting | rubash/bash-like | Stay rubash-owned; do not parse zsh syntax in winuxsh |
| Script execution | Whole-script parse/execute | Done | Keep expanding compat fixtures |
| Command discovery | PATH-aware external command discovery | Partial | Keep winuxcmd-first, improve executable suffix discovery carefully |
| Coreutils | Built-in/internal command set | Provided by winuxcmd | Expand completion/help coverage, not FFI |
| Windows path input | Native drive paths in commands | `C:/...` and obvious `C:\...` inputs work | Accept native Windows drive literals at host boundary without adopting MSYS path authority |
| Completion providers | `_cmd`, `#compdef`, aliases, flags, values, paths | Partial | Import safe zsh completion assets into native TOML/provider model |
| Completion matching | zstyle matcher-list, grouping, cache, case sensitivity | Configurable prefix/substring, case sensitivity, command cap | Keep this native surface small; defer fuzzy/zstyle matcher-list translation |
| External completer bridge | Carapace-like external completion command | Not started | v2.3/v3 candidate, secondary to zsh asset import |
| History | path, size, sync, ignore-space, SQLite/isolation | Configurable plaintext file | Keep `[history]` small; defer sync/isolation until usage proves it |
| Edit mode | `bindkey -e/-v`, mode indicators, cursor shape | Emacs/Vi done | Import simple bindkey mode and add prompt/cursor polish |
| Keybindings | ZLE widgets and bindkey maps | Common built-in ZLE widgets mapped to reedline | Keep native subset conservative; arbitrary ZLE plugin scripts are deferred |
| Menus | Completion/history menu config | Configurable page size and max entry lines | Keep native menu controls small; defer zstyle menu/select/group/order translation |
| Prompt | `PROMPT`, `RPROMPT`, `%~`, `%F{}`, Git segments | Left prompt template | Translate common zsh prompt/theme forms into native config |
| Theme | Oh My Zsh themes and native color config | Prompt/status colors only | Add theme translator before online theme market |
| Plugins | Oh My Zsh `plugins=(...)` and `$ZSH_CUSTOM` | Not started | Import completion/alias/theme assets first; native modules for editor UX |
| Package layer | Oh My Zsh-style plugins/themes/completions | Not started | Oh-My-Winuxsh starts as local compatibility/package layer |
| Job control | jobs/fg/bg/kill | Rubash-owned | Validate host wiring only; do not reimplement in winuxsh |
| Terminal integration | OSC/Kitty protocol hints | Not started | Optional polish, after core UX |

## Recommended Next Functional Phases

### Phase A - v2.2 Closeout

- Fix CI workflow warnings:
  - use supported Rust toolchain action inputs
  - update checkout action if needed
- Audit `tar` source and add completion only after source is known.
- Expand winuxcmd completion definitions for remaining high-use commands from
  `winuxcmd.exe <cmd> --help`.
- Mark v2.2 complete only after docs, tests, and master CI are green.

### Phase B - v2.3 Shell UX Baseline

- Lock down the Windows-native terminal contract:
  - `~`/home maps to the normal Windows user profile, not an isolated Unix root
  - process env, PATH, cwd, stdout, stderr, and exit code are native Windows process state
  - non-interactive mode stays quiet and agent-friendly
  - no PowerShell alias/wildcard behavior leaks into execution
- Add `[history]` config:
  - `path`
  - `max_size`
  - `ignore_space_prefixed`
  - maybe `sync_on_enter` if reedline/FileBackedHistory makes it practical
- Add prompt polish:
  - right prompt
  - vi/emacs prompt indicators
  - multiline indicator config
- Add completion behavior config:
  - case sensitivity
  - prefix vs substring matching
  - max external command results
- Add command-not-found UX:
  - clear message
  - optional suggestion from PATH/winuxcmd command list

### Phase C - v2.4 Zsh Compatibility Foundation

- Add a safe zsh profile scanner:
  - discover `ZDOTDIR` and `.zshrc`
  - import aliases, exports, `plugins=(...)`, `ZSH_THEME`, `bindkey -e/-v`
  - record unsupported lines as diagnostics
- Add Oh My Zsh layout import for completion-only, alias-only, and theme assets.
- Add zsh-like interactive conveniences that do not affect bash semantics:
  - autosuggestions/hints from history
  - richer completion menu behavior
  - configurable prompt indicators
  - optional right prompt
  - native syntax highlighting
- Keep implementation in reedline/config/prompt/completion layers.
- Do not add shell-language features in winuxsh itself.

### Phase D - v3 Design Before Code

- Plugin contract ADR:
  - process-based protocol first
  - no parser/executor extension
  - no Rust ABI dependency
- Oh-My-Winuxsh local package shape:
  - themes
  - completion packs
  - prompt snippets
  - plugin binaries later
- Rubash capability validation note:
  - verify which bash upstream tests are already covered by rubash
  - add winuxsh host-level smoke tests only where embedding changes behavior
  - upstream fixes to rubash when semantics fail; do not fork behavior in winuxsh

## Current Position Statement

Winuxsh is **not trying to become zsh.exe or Nushell**. zsh/Oh My Zsh are the
primary references for interactive comfort and ecosystem compatibility, while
Nushell is a secondary reference for modern shell ergonomics. Winuxsh should
become the comfortable Windows-native bash/zsh terminal that humans and agents
can both use directly:

- rubash-compatible where scripts and semantics matter
- winuxcmd-backed where Unix tools matter
- reedline-modern where interactive UX matters
- Windows-native and non-isolated where process/env/home behavior matters
- quiet, deterministic, and testable where agents depend on it
