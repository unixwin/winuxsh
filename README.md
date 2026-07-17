# Winuxsh

[中文](README-zh.md) | English

A bash-compatible shell for Windows, built on [rubash](https://github.com/unixwin/rubash) + [winuxcmd](https://github.com/unixwin/WinuxCmd).

Winuxsh does not implement its own shell language. Instead it acts as the Windows-facing interactive layer on top of `rubash` (the bash-compatible engine) and routes Unix utility calls through `winuxcmd.exe` via `PATH` injection. Winuxsh itself owns the experience layer: reedline REPL, completion system, theming, configuration, Ctrl+C handling, and Windows integration.

## Architecture

```
winuxsh.exe
├── winuxsh runtime (Rust)
│   ├── rubash::Executor   ← shell language engine (lexer/parser/execution/builtins)
│   ├── reedline REPL      ← line editing, history, completion popup
│   ├── completion system  ← TOML + bash auto-import + on-disk cache
│   ├── theme/prompt       ← 4 built-in themes, template prompts
│   ├── config             ← ~/.winshrc.toml
│   └── ctrl_c             ← Win32 Ctrl+C handler
├── rubash lib
│   ├── lexer/parser/ast
│   ├── executor (pipeline/redirect/alias/function/array)
│   └── builtins (cd/source/export/set/test/printf/...)
└── winuxcmd.exe           ← Unix coreutils (ls/cat/grep/find/cp/mv/rm/...)
```

Winuxcmd is integrated by prepending its directory to `PATH` at startup; rubash's existing `find_user_command` picks it up first.

## Build

Prerequisites:

- Rust 1.70+
- `winuxcmd.exe` reachable from `PATH` (or via `WINUXCMD_PATH` env var, or co-located in `./winuxcmd/` next to `winuxsh.exe`)

```pwsh
cargo build --release
# binary at target/release/winuxsh.exe
```

## Usage

```
winuxsh                       # interactive REPL
winuxsh -c "command"          # execute a single command and exit
winuxsh script.sh [args...]    # run a script file
winuxsh -h | --help
winuxsh -V | --version
```

Inside the shell, you have full bash semantics via rubash, plus completion via the winuxsh completion system:

```bash
echo $PATH                       # variable expansion
ls -la                           # via winuxcmd (PATH-injected)
echo "today: $(date +%Y)"        # command substitution
for i in 1 2 3; do echo $i; done # control flow
cat Cargo.toml | grep name       # pipeline builtin + external
```

## Configuration

`~/.winshrc.toml`:

```toml
[shell]
prompt_format = "{user}@{host} {cwd} {symbol}"

[theme]
current_theme = "default"      # default | dark | light | colorful | ~/.winuxsh/themes/<name>.toml

[editor]
edit_mode = "emacs"          # emacs | vi

[aliases]
ll = "ls -la"
la = "ls -a"

[completions]
# directories containing <cmd>.toml or <cmd>.bash completion scripts
completion_dirs = [
    "D:/shellTools/ripgrep/complete",
    "D:/shellTools/fd/autocomplete",
]

[winuxcmd]
# optional override; auto-detected from PATH if not set
# path = "D:/tools/winuxcmd/winuxcmd.exe"
```

Custom themes live in `~/.winuxsh/themes/<name>.toml` and are selected with
`[theme].current_theme = "<name>"`. Built-in names (`default`, `dark`, `light`,
`colorful`) remain reserved and stable.

```toml
[prompt_user]
fg = "light cyan"
bold = true

[prompt_host]
fg = "white"
bold = true

[prompt_dir]
fg = "light green"
bold = true

[prompt_symbol]
fg = "light magenta"
bold = true

[error]
fg = "red"
bold = true

[warning]
fg = "yellow"

[success]
fg = "green"
```

## Completion system

Winuxsh inherits the MVP6 completion stack:

- **TOML definitions** — drop `<cmd>.toml` into any `completion_dirs` entry
- **Bash auto-import** — `_cmd.bash` / `cmd.bash` files get parsed for `opts="..."`
- **Auto-description** — `cmd -h` is run once per command to populate flag descriptions, cached on disk
- **Three-layer cache** — in-memory → `.parsed.toml` on disk → subprocess
- **ListMenu popup** — aligned descriptions, shown for `cmd -<Tab>`, file paths, env vars

## Status

This branch (`rewrite/v2-rubash`) is v1: core REPL + completion + theme + config. Vi mode and `Ctrl+R` history search are available in v2.2; the plugin framework and Oh-My-Winuxsh are tracked for later iterations.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
