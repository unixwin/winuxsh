---
tags: [winuxsh, roadmap, v3, design]
created: 2026-07-17
status: draft
---

# Winuxsh v3 Design Plan

> Goal: after v2.2/v2.3 stabilizes the Windows-native bash/zsh terminal
> contract, plan v3 extension features without weakening the locked shell
> architecture.

## Locked Architecture

- Shell language semantics stay in `rubash`.
- Coreutils stay in `winuxcmd.exe` through PATH injection.
- REPL/frontend behavior stays in `reedline`.
- Config remains backward compatible with `~/.winshrc.toml`.
- History remains `~/.winuxsh_history`.
- `~` and default config/history locations map to the normal Windows user home,
  not to an isolated Unix environment.
- Winuxsh inherits and mutates the real Windows process environment.
- Do not restore the old winsh lexer/parser/core/ast stack.
- Do not reintroduce winuxcmd FFI/DLL integration.
- Do not adopt Nushell syntax, pipeline semantics, or data model.
- Do not duplicate rubash-owned builtins or job-control semantics in winuxsh.

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

### Rubash Capability Validation

- Treat shell semantics, builtins, and job-related behavior as rubash-owned.
- Validate winuxsh only as an embedding host:
  - PATH injection before executor construction
  - real Windows env/cwd/home behavior
  - stdout/stderr/exit-code propagation
  - Ctrl+C and console interaction boundaries
- If semantics fail, prefer upstream rubash fixes over winuxsh-side behavior forks.
- Add winuxsh smoke tests only where embedding can change rubash behavior.

## Proposed Order

1. Write a plugin contract ADR and identify the minimum stable host API.
2. Prototype process-based plugin discovery outside the execution path.
3. Package user themes and completion definitions as local Oh-My-Winuxsh bundles.
4. Add a rubash capability validation matrix for host-level smoke tests.
5. Implement only after each track has tests and rollback boundaries.

## Test Gate

- Keep the v2.2 baseline green before each v3 change:
  - `cargo fmt --check`
  - `cargo test --lib -p winuxsh-runtime --locked`
  - `cargo test -p winuxsh-runtime --test completion --locked`
  - `cargo build --locked`
  - `cargo test --test compat -- --ignored`
- Add feature-specific tests before enabling user-facing v3 behavior.
