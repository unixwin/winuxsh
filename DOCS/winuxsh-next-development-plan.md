---
tags: [winuxsh, roadmap, native-windows, agent-terminal, v2.3]
created: 2026-07-17
status: active
---

# Winuxsh Next Development Plan

> Updated product frame: winuxsh is a Windows-native, non-isolated, bash/zsh-like
> terminal for both users and agents. It is not MSYS2, Git Bash, Cygwin, WSL,
> PowerShell compatibility mode, or Nushell semantics.

## Development Principles

- Keep shell syntax, parser, executor, builtins, and job-related semantics in rubash.
- Keep coreutils in `winuxcmd.exe`, discovered through PATH injection.
- Keep the terminal process native to Windows:
  - no filesystem root isolation
  - `~` means the Windows user home
  - env/PATH/cwd/stdout/stderr/exit-code are normal Windows process state
- Keep non-interactive behavior agent-friendly:
  - no banners
  - stable stdout/stderr separation
  - exact exit code propagation
  - deterministic `-c` and script execution
- Use zsh/Oh My Zsh as the primary UX and compatibility reference:
  completion, history, prompt, menus, themes, packages, `.zshrc`, and
  plugin-layout ergonomics.
- Use Nushell only as a secondary reference for modern config/menu design and
  reedline integration, not shell language or pipeline semantics.

## Phase 0 - Hygiene Before More Features

- Fix CI warnings:
  - replace unsupported `rust-version` input with the setup action's supported
    `toolchain` input
  - update checkout action if needed to remove Node deprecation warning
- Push the corrected positioning docs after review.
- Keep `.tmp/` and external reference clones out of commits.

Verification:

- `cargo fmt --check`
- `cargo build --locked`
- `cargo test --lib -p winuxsh-runtime --locked`
- GitHub Actions master CI green without avoidable workflow warnings.

## Phase 1 - Windows Native Terminal Contract

- Add host-level tests or smoke fixtures for:
  - `~` / home resolution uses Windows user home
  - cwd is inherited and updated natively
  - PATH injection preserves existing Windows PATH entries and prepends winuxcmd
  - stdout, stderr, and exit code behave predictably in `-c`
  - script mode stays quiet and deterministic for agents
- Document the native contract in README and architecture docs.
- Avoid any MSYS2/Git Bash/WSL path translation layer.

Verification:

- Existing compat suite remains green.
- New host-contract tests run without requiring PowerShell aliases.

## Phase 2 - v2.2 Completion Closeout

- Audit `winuxcmd.exe --list` and each high-use `winuxcmd.exe <cmd> --help`.
- Decide `tar` source before adding `tar.toml`.
- Continue built-in completion coverage for remaining stable winuxcmd commands.
- Keep definitions generated or audited from winuxcmd help output, not guessed.
- Preserve user TOML override priority over built-ins.

Verification:

- `cargo test -p winuxsh-runtime --test completion --locked`
- Add tests for each new command family.

## Phase 3 - Agent-Friendly Error and Command UX

- Improve command-not-found output:
  - concise message
  - no PowerShell suggestions
  - optional nearest winuxcmd/PATH suggestion
- Ensure external command failures preserve the command's stderr.
- Avoid extra friendly text in non-interactive mode unless explicitly requested.
- Add docs for agent usage patterns:
  - `winuxsh -c`
  - script execution
  - expected exit-code behavior

Verification:

- CLI tests for command-not-found and non-zero exit behavior.

## Phase 4 - Zsh Compatibility Foundation

- Make zsh the primary UX compatibility reference.
- Add a zsh profile scanner before implementing broad plugin compatibility.
  The scanner reads and translates, but does not execute arbitrary zsh scripts:
  - `.zshrc`
  - safe `.zshenv` assignments
  - `ZDOTDIR`
  - `plugins=(...)`
  - `ZSH_THEME`
  - simple aliases/exports/path/fpath/zstyle/bindkey
- Add an explicit import/report command path before automatic startup import:
  - `winuxsh --import-zsh` or equivalent CLI surface
  - dry-run diagnostics for unsupported zsh/ZLE/plugin constructs
  - native `.winshrc.toml` suggestions for stable settings
- Add `[history]` config:
  - `path`
  - `max_size`
  - `ignore_space_prefixed`
- Add prompt polish:
  - right prompt
  - vi/emacs prompt indicators
  - configurable multiline indicator
- Add completion UX config:
  - case sensitivity
  - prefix vs substring matching
  - max external command results
- Implement autosuggestions natively in reedline, honoring common
  `ZSH_AUTOSUGGEST_*` settings where practical.
- Implement syntax highlighting natively with rubash/reedline, honoring a
  useful subset of `ZSH_HIGHLIGHT_STYLES`.

Verification:

- Runtime unit tests for config parsing.
- REPL keybinding tests preserve Tab completion and Ctrl+R.

## Phase 5 - Packaging and Defaults

- Provide a Windows Terminal profile recommendation.
- Provide a minimal default `.winshrc.toml` optimized for users and agents.
- Keep config backward compatible.
- Add Oh My Zsh layout import before designing any online Oh-My-Winuxsh registry:
  - completion assets
  - alias-only plugins
  - themes/prompt snippets
  - native autosuggestion/highlighting modules
- Keep direct zsh plugin compatibility tiered:
  - import completion-only and alias-only plugins first
  - translate simple prompt/theme snippets second
  - reimplement autosuggestions/highlighting natively
  - report and skip arbitrary ZLE/zmodload/zpty plugins

Verification:

- Fresh checkout/build instructions work on Windows.
- No required MSYS2/Git Bash/WSL dependency appears in docs or scripts.

## Phase 6 - v3 Design Gate

- Plugin framework remains a v3 design topic, but only for UX/package extension:
  - completion providers
  - prompt/theme providers
  - helper commands as external processes
- Do not design plugins that extend rubash parser/executor.
- Build a rubash capability matrix before considering any host-level semantic
  work.

Definition of ready for v3:

- v2.2/v2.3 native terminal contract is documented and tested.
- winuxcmd completion coverage is broad enough for daily use.
- agent non-interactive behavior is stable.
- master CI is green and warning-light.
