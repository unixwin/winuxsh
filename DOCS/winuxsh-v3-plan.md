---
tags: [winuxsh, roadmap, v3, design]
created: 2026-07-17
status: draft
---

# Winuxsh v3 Design Plan

> Goal: after v2.2 stabilizes the rubash + winuxcmd + reedline rewrite, plan
> v3 features without weakening the locked shell architecture.

## Locked Architecture

- Shell language semantics stay in `rubash`.
- Coreutils stay in `winuxcmd.exe` through PATH injection.
- REPL/frontend behavior stays in `reedline`.
- Config remains backward compatible with `~/.winshrc.toml`.
- History remains `~/.winuxsh_history`.
- Do not restore the old winsh lexer/parser/core/ast stack.
- Do not reintroduce winuxcmd FFI/DLL integration.
- Do not adopt Nushell syntax, pipeline semantics, or data model.

## v3 Tracks

### Plugin Framework

- Define plugin goals before choosing runtime mechanics:
  - add completion providers
  - add prompt/theme providers
  - add shell helper commands that execute as external processes
  - expose safe metadata about cwd, env, aliases, and last exit code
- Prefer process-based plugins first because they preserve the current binary
  boundary and avoid Rust ABI instability.
- Evaluate WASI/WASM only after the process plugin contract is documented.
- Keep parser/executor extension out of scope unless rubash exposes a stable
  upstream extension point.

### Oh-My-Winuxsh

- Treat this as packaging around existing extension points, not a new shell
  runtime.
- First package types:
  - themes under `~/.winuxsh/themes/*.toml`
  - completion definitions under user `completion_dirs`
  - prompt templates in config snippets
- Defer online marketplace behavior until local package install/update/remove
  semantics are clear.

### Job Control

- Start with design research because Windows process groups, console control
  events, and POSIX job control do not map 1:1.
- Target user-visible commands:
  - `jobs`
  - `fg`
  - `bg`
  - `kill`
- Keep shell syntax and command parsing in rubash; winuxsh should only own the
  Windows process-control bridge if rubash cannot provide it directly.
- Build a compat fixture plan before implementation because job control is
  high risk and interactive.

## Proposed Order

1. Write a plugin contract ADR and identify the minimum stable host API.
2. Prototype process-based plugin discovery outside the execution path.
3. Package user themes and completion definitions as local Oh-My-Winuxsh bundles.
4. Research Windows job-control primitives and rubash integration seams.
5. Implement only after each track has tests and rollback boundaries.

## Test Gate

- Keep the v2.2 baseline green before each v3 change:
  - `cargo fmt --check`
  - `cargo test --lib -p winuxsh-runtime --locked`
  - `cargo test -p winuxsh-runtime --test completion --locked`
  - `cargo build --locked`
  - `cargo test --test compat -- --ignored`
- Add feature-specific tests before enabling user-facing v3 behavior.

