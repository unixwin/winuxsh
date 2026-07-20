# Winuxsh

[中文](README-zh.md) | English

A Windows-native shell that feels like bash and zsh, without MSYS2, Git Bash,
Cygwin, WSL, or PowerShell semantics. Built for humans and coding agents.

```text
caomengxuan@DESKTOP C:\Users\me\repo\winuxsh  git:(master) ●2 ✚1 ↑1 ?3
%
```

That prompt shows branch, dirty state, staged/unstaged counts, ahead/behind,
and untracked files - the oh-my-zsh `git_prompt_status` flavor, but built
natively in Rust. It works the moment you `cd` into a git repo.

## What it is

```text
winuxsh = rubash shell engine + winuxcmd coreutils + reedline frontend
```

- `rubash` provides the shell language: parsing, executing, builtins,
  pipelines, redirects, aliases, functions, and job semantics.
- `winuxcmd.exe` provides Unix coreutils (`ls`, `cat`, `grep`, `find`, `cp`,
  `mv`, `rm`, `mkdir`) through Windows `PATH` injection.
- `reedline` provides the interactive frontend: editing, history, completion
  menus, hints, and prompt rendering.
- `~` is the normal Windows user home. `PATH`, environment, cwd, stdout,
  stderr, and exit code are normal Windows process state.

Winuxsh is not `zsh.exe`. It reads your `.zshrc` and Oh My Zsh intent where
safe, then implements the useful pieces natively in Rust.

## Why use it

If you've ever opened PowerShell to do `ls` and got back a property sheet, or
watched a coding agent fail because `test -f Cargo.toml` doesn't exist in
`pwsh`, winuxsh is the answer. It is for people who want bash fluency on
Windows and want agents to run the same commands they'd run on Linux.

What winuxsh gives you:

- A real bash-compatible shell on Windows, with `if`, `for`, `while`, `case`,
  functions, heredocs, pipes, redirects, and `$(...)`.
- Native Windows process behavior: no fake `/c` filesystem, no MSYS isolation,
  no PowerShell wildcard surprises. `cd D:\repo` and `pwd` prints
  `D:/repo` like Windows native.
- Zsh-like interactive comfort: autosuggestions, syntax highlighting, Vi or
  Emacs editing, Ctrl+R history search, multiline PS2 prompts, and rich Tab
  completion.
- An agent-friendly CLI: `winuxsh -c ...` and `winuxsh script.sh` are quiet,
  deterministic, and preserve stdout, stderr, and exit code exactly.
- A safe path into your existing `.zshrc` and Oh My Zsh setup: winuxsh scans
  them, prints a review plan, and only writes when you say so.

## See it work

Build from source:

```pwsh
cargo build --release
```

Start the interactive shell:

```pwsh
target\release\winuxsh.exe
```

Try a real prompt:

```bash
cd C:\Users\me\repo\winuxsh
pwd                          # prints C:/Users/me/repo/winuxsh
git status                   # the prompt picks up branch + dirty state
ls -la                       # Unix-style listing from winuxcmd
echo $(git rev-parse --short HEAD)
for i in 1 2 3; do echo $i; done
if [ -f Cargo.toml ]; then echo "is a rust project"; fi
```

Native Windows paths work as shell input:

```bash
ls C:\Users
ls C:/Users/YourName         # both styles work
cd C:/Users/YourName/repo
```

Multiline commands wait for the block to complete, like a normal shell:

```bash
HTTP_CODE=200
if [ $HTTP_CODE -eq 200 ]; then
  echo "OK"
fi
```

## Zsh users: start here

Don't copy plugin source into winuxsh. Let winuxsh inspect your existing
setup, then decide what to import:

```pwsh
winuxsh --zsh-compat-report         # human-readable scan of ~/.zshrc
winuxsh --zsh-compat-report-json    # same, machine-readable
winuxsh --zsh-compat-import-plan    # what would land in ~/.winshrc.toml
```

If the plan looks right, apply it explicitly:

```pwsh
winuxsh --zsh-compat-import-apply
winuxsh --zsh-compat-import-status
winuxsh --zsh-compat-doctor
```

Rollback is review-first:

```pwsh
winuxsh --zsh-compat-import-rollback-plan
```

For a low-risk starter profile or a deterministic agent profile:

```pwsh
winuxsh --zsh-profile-plan zsh-lite
winuxsh --zsh-profile-plan agent
```

Detailed walkthrough: [Zsh Migration Guide](DOCS/zsh-migration-guide.md).

## Built-in zsh-style packs

Winuxsh ships native support for common zsh / Oh My Zsh plugin behavior. The
packs are implemented in Rust on top of rubash, reedline, and winuxcmd. No
zsh plugin source is vendored and no zsh scripts are sourced at startup.

Inspect the inventory:

```pwsh
winuxsh --zsh-native-packs
winuxsh --zsh-native-packs-json
```

Default-on packs (safe UI features):

- `zsh-autosuggestions` - history-based inline suggestions
- `zsh-syntax-highlighting` - main highlighter subset

Opt-in or profile-planned packs:

- `git` - Oh My Zsh-style aliases (`g`, `gst`, `gco`, `gl`, `gp`, `glog`) plus
  the colored prompt status you saw at the top of this README
- `zsh-history-substring-search`, standard ZLE widget mappings
- `docker`, `kubectl`, `npm`
- `command-not-found`, `direnv`, `dotenv`, `zoxide`, `thefuck`, `fzf`,
  `zsh-interactive-cd`, `last-working-dir`

Lifecycle and external-command packs stay disabled until you opt in via
`~/.winshrc.toml`, because they can read project files, run external
commands, or change shell state.

## Configuration

`~/.winshrc.toml` is the native control plane. It is explicit, rollback-safe,
and easy for humans and agents to inspect. `.zshrc` is read for import only -
the runtime authority is the TOML.

Minimal example:

```toml
[shell]
prompt_format = "{user}@{host} {cwd} {git_prompt} {symbol}"
right_prompt_format = ""
multiline_indicator = "> "

[editor]
edit_mode = "emacs" # emacs | vi

[history]
path = "~/.winuxsh_history"
max_size = 10000
ignore_space_prefixed = true

[theme]
current_theme = "default" # default | dark | light | colorful | ~/.winuxsh/themes/<name>.toml

[aliases]
ll = "ls -la"
la = "ls -a"

[completions]
matching = "prefix" # prefix | substring
case_sensitive = false
max_command_results = 500
completion_dirs = []

[menus]
completion_page_size = 10
history_page_size = 10
max_entry_lines = 5

[winuxcmd]
# Optional override; auto-detected from PATH if omitted.
# path = "D:/tools/winuxcmd/winuxcmd.exe"
```

The `{git_prompt}` placeholder shows your branch plus compact status symbols
when you're inside a git repo, and stays empty when you're not. Symbols:
`●N` staged, `✚N` modified, `?N` untracked, `↑N` ahead, `↓N` behind,
`⚑N` stashes, `✖N` conflicts. Branch color flips from green to yellow as
soon as the tree is dirty.

Zsh compat import writes a managed block into the same TOML file only when
you explicitly run `--zsh-compat-import-apply`.

## What runs through rubash

Shell language goes through rubash. This covers:

- variables and command substitution
- pipes and redirects
- aliases and functions
- `if`, `for`, `while`, `case`, functions, heredoc, and line continuations
- script files and `-c` whole-script execution
- multiline compound commands collected into one execution unit

Job control and the executable grammar are owned by rubash; winuxsh does not
reimplement them. That keeps winuxsh small and lets shell-level semantics
benefit from rubash's upstream bash test coverage.

## Completion system

Winuxsh completion supports:

- built-in winuxcmd command definitions (`ls`, `grep`, `find`, `cat`, `cp`,
  `mv`, `rm`, `mkdir`, `touch`, `chmod`, and now `git`)
- `git` subcommand awareness: `git add <Tab>`, `git commit -<Tab>`,
  `git push --<Tab>` all get real candidates
- command completion from Windows `PATH` and `PATHEXT`
- path completion with quoting/escaping for spaces
- environment variable completion
- user TOML definitions from `completion_dirs`
- safe imported zsh completion assets where they can be translated
- explicit dynamic/runtime completion providers with allowlists and timeouts

User TOML definitions override built-ins, so your `~/.winuxsh/completions/ls.toml`
wins over the bundled `ls.toml`.

## Architecture

```text
winuxsh.exe
├── winuxsh runtime (Rust)
│   ├── rubash::Executor        shell language engine
│   ├── reedline REPL           editing, history, menus, hints
│   ├── completion system       TOML, zsh import, cache, PATH commands
│   ├── zsh compatibility       scanner, import plan, native packs
│   ├── theme/prompt            templates, built-in and user themes
│   ├── git status              branch + counts via `git status --porcelain`
│   ├── config                  ~/.winshrc.toml
│   └── ctrl_c                  Win32 Ctrl+C handling
├── rubash lib                  parser/executor/builtins
└── winuxcmd.exe                Unix coreutils through PATH injection
```

## Non-goals

- No PowerShell semantics or wildcard behavior.
- No MSYS2 / Git Bash / Cygwin / WSL isolation.
- No Nushell syntax or structured pipeline model.
- No zsh parser or ZLE runtime inside winuxsh.
- No blind startup sourcing of `.zshrc` or zsh plugin scripts.
- No `winuxcmd` FFI/DLL integration.
- No reimplementation of the rubash parser/executor or core shell semantics.

## Documentation

- [Getting Started](DOCS/getting-started.md)
- [Zsh Migration Guide](DOCS/zsh-migration-guide.md)
- [Roadmap](DOCS/winuxsh-roadmap.md)
- [Architecture](DOCS/architecture.md)
- [Native Zsh Plugin Pack Plan](DOCS/winuxsh-native-zsh-plugin-pack-plan.md)
- [Zsh Compatibility Plan](DOCS/zsh-compatibility-plan.md)
- [Positioning and Feature Map](DOCS/winuxsh-positioning-and-feature-map.md)

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
