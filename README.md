# Winuxsh

[中文](README-zh.md) | English

Winuxsh is a Windows-native bash/zsh-style shell for humans and coding agents.
It gives Windows users a Unix shell experience without MSYS2, Git Bash, Cygwin,
WSL isolation, or PowerShell command semantics.

Core formula:

```text
winuxsh = rubash shell engine + winuxcmd coreutils + reedline frontend
```

That means:

- `rubash` owns shell syntax, parsing, execution, builtins, pipelines, redirects,
  aliases, functions, and job-related shell semantics.
- `winuxcmd.exe` provides Unix-style tools such as `ls`, `cat`, `grep`, `find`,
  `cp`, `mv`, `rm`, and `mkdir` through Windows `PATH` injection.
- `reedline` provides the interactive frontend: editing, history, completion
  menus, hints, and prompt rendering.
- `~` is the normal Windows user home. `PATH`, environment variables, cwd,
  stdout, stderr, and exit code are normal Windows process state.

Winuxsh is not trying to be `zsh.exe`. It is a native Windows shell that reads
zsh/Oh My Zsh intent where safe, then implements the useful pieces natively.

## Why Winuxsh

Windows already has PowerShell, but many users and agents want a bash/zsh-like
terminal that behaves predictably across projects and scripts.

Winuxsh focuses on this product contract:

- **Unix shell syntax on Windows**: use bash-compatible syntax through rubash.
- **Native Windows process behavior**: no isolated filesystem root, no `/c` path
  authority, no PowerShell wildcard or alias execution surprises.
- **Zsh-like interactive comfort**: autosuggestions, syntax highlighting, Vi or
  Emacs editing, Ctrl+R history search, multiline PS2 prompts, and rich Tab
  completion.
- **Agent-friendly execution**: `winuxsh -c ...` and script files are quiet,
  deterministic, and preserve stdout/stderr/exit codes.
- **Safe zsh compatibility**: scan and translate `.zshrc` / Oh My Zsh assets;
  do not blindly source arbitrary zsh plugin scripts at startup.

## Status

Current `master` is the rubash-backed rewrite with:

- interactive REPL and non-interactive `-c` / script execution
- whole-script execution for heredoc, continuation, multiline `if` / `for` /
  functions, and bash smoke fixtures
- Windows-native cwd/path contract with visible `C:/...` paths by default
- built-in winuxcmd completions for common coreutils
- command and path completion, including blank Tab and `gre<Tab>` command prefix
  completion
- Emacs/Vi edit modes, Ctrl+R history search, configurable history and menus
- native autosuggestions and syntax highlighting
- user themes from `~/.winuxsh/themes/*.toml`
- safe `.zshrc` / Oh My Zsh scanner, report, import plan, explicit apply,
  status, rollback plan, and doctor commands
- native zsh-style packs for Git, Docker, kubectl, npm, autosuggestions,
  syntax highlighting, history widgets, direnv, dotenv, zoxide, thefuck,
  command-not-found, fzf, interactive-cd, and last-working-dir

Some profile commands, such as `--zsh-profile-plan zsh-lite`, are planned but
not implemented yet. Use the current compatibility commands below for now.

## Quick Start

Build from source:

```pwsh
cargo build --release
```

Run interactively:

```pwsh
target\release\winuxsh.exe
```

Run a command or script:

```pwsh
winuxsh -c "pwd; ls -la; git status"
winuxsh script.sh arg1 arg2
```

Inside winuxsh:

```bash
pwd
ls -la
echo "home is $HOME"
for i in 1 2 3; do echo $i; done
if [ -f Cargo.toml ]; then echo ok; fi
cat Cargo.toml | grep name
```

Native Windows paths work as shell input:

```bash
ls C:/Users
ls C:\Users\YourName
cd C:/Users/YourName/repo
pwd
```

`pwd` intentionally prints Windows-native `C:/...` paths by default. Legacy
`/c/...` input is accepted as compatibility input, but it is not the default path
style and not a fake MSYS root.

## Zsh Users: Start Here

If you already have a `.zshrc`, do not copy random plugin source into winuxsh.
Let winuxsh inspect it first:

```pwsh
winuxsh --zsh-compat-report
winuxsh --zsh-compat-report-json
winuxsh --zsh-compat-import-plan
```

If the import plan looks good, apply it explicitly:

```pwsh
winuxsh --zsh-compat-import-apply
winuxsh --zsh-compat-import-status
winuxsh --zsh-compat-doctor
```

Rollback is review-first:

```pwsh
winuxsh --zsh-compat-import-rollback-plan
```

See the detailed guide: [Zsh Migration Guide](DOCS/zsh-migration-guide.md).

## Native Zsh Packs

Winuxsh ships native support for common zsh / Oh My Zsh plugin behavior. These
are built into winuxsh as Rust/reedline/rubash integrations; zsh plugin scripts
are not vendored and are not sourced at startup.

Inspect the current inventory:

```pwsh
winuxsh --zsh-native-packs
winuxsh --zsh-native-packs-json
```

Default-on packs are limited to safe interactive UI features:

- `zsh-autosuggestions`: native history-based inline suggestions
- `zsh-syntax-highlighting`: native main highlighter subset

Useful but opt-in or profile-planned packs include:

- `git`: Oh My Zsh-style aliases such as `g`, `gst`, `gco`, `gl`, `gp`, `glog`
- `zsh-history-substring-search` and standard ZLE widget mappings
- `docker`, `kubectl`, `npm`
- `command-not-found`, `direnv`, `dotenv`, `zoxide`, `thefuck`, `fzf`,
  `zsh-interactive-cd`, `last-working-dir`

Lifecycle or external-command packs remain disabled until explicitly enabled in
`~/.winshrc.toml` because they can read project files, run external commands, or
change shell-visible state.

## Configuration

Winuxsh uses `~/.winshrc.toml` as its native control plane. `.zshrc` remains a
familiar import source, but the runtime authority is TOML because it is explicit,
rollback-safe, and easy for humans and agents to inspect.

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

Zsh compatibility import writes a managed block into the same TOML file only
when you explicitly run `--zsh-compat-import-apply`.

## Shell Behavior

Winuxsh executes shell language through rubash. This includes:

- variables and command substitution
- pipes and redirects
- aliases and functions
- `if`, `for`, `while`, `case`, functions, heredoc, and continuations
- script files and `-c` whole-script execution

Interactive multiline input behaves like a normal shell:

```bash
HTTP_CODE=200
if [ $HTTP_CODE -eq 200 ]; then
  echo OK
fi
```

The REPL waits for the command block to complete and then executes it once.

## Completion System

Winuxsh completion supports:

- built-in winuxcmd command definitions
- command completion from Windows `PATH` and `PATHEXT`
- path completion with quoting/escaping for spaces
- environment variable completion
- user TOML definitions from `completion_dirs`
- safe imported zsh completion assets where they can be translated
- explicit dynamic/runtime completion providers with allowlists and timeouts

User TOML definitions override built-ins.

## Architecture

```text
winuxsh.exe
├── winuxsh runtime (Rust)
│   ├── rubash::Executor        shell language engine
│   ├── reedline REPL           editing, history, menus, hints
│   ├── completion system       TOML, zsh import, cache, PATH commands
│   ├── zsh compatibility       scanner, import plan, native packs
│   ├── theme/prompt            templates, built-in and user themes
│   ├── config                  ~/.winshrc.toml
│   └── ctrl_c                  Win32 Ctrl+C handling
├── rubash lib                  parser/executor/builtins
└── winuxcmd.exe                Unix coreutils through PATH injection
```

## Non-Goals

- no PowerShell semantics or wildcard behavior
- no MSYS2/Git Bash/Cygwin/WSL isolation
- no Nushell syntax or structured pipeline model
- no zsh parser or ZLE runtime inside winuxsh
- no blind startup sourcing of `.zshrc` or zsh plugin scripts
- no winuxcmd FFI/DLL integration
- no reimplementation of rubash parser/executor/core shell semantics

## Documentation

- [Zsh Migration Guide](DOCS/zsh-migration-guide.md)
- [Roadmap](DOCS/winuxsh-roadmap.md)
- [Architecture](DOCS/architecture.md)
- [Native Zsh Plugin Pack Plan](DOCS/winuxsh-native-zsh-plugin-pack-plan.md)
- [Zsh Compatibility Plan](DOCS/zsh-compatibility-plan.md)
- [Positioning and Feature Map](DOCS/winuxsh-positioning-and-feature-map.md)

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).