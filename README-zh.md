# Winuxsh

中文 | [English](README.md)

Winuxsh 是面向人类用户和 coding agents 的 Windows 原生 bash/zsh-style shell。
它希望在 Windows 上提供熟悉的 Unix shell 体验，但不依赖 MSYS2、Git Bash、
Cygwin、WSL 隔离层，也不采用 PowerShell 命令语义。

核心公式：

```text
winuxsh = rubash shell engine + winuxcmd coreutils + reedline frontend
```

这意味着：

- `rubash` 负责 shell 语法、解析、执行、内建命令、管道、重定向、alias、
  function 和 job 相关 shell 语义。
- `winuxcmd.exe` 通过 Windows `PATH` 注入提供 `ls`、`cat`、`grep`、`find`、
  `cp`、`mv`、`rm`、`mkdir` 等 Unix-style 工具。
- `reedline` 负责交互式前端：编辑、历史、补全菜单、提示和 prompt 渲染。
- `~` 就是普通 Windows 用户 home。`PATH`、环境变量、cwd、stdout、stderr、
  exit code 都保持 Windows 原生进程状态。

Winuxsh 不想成为 `zsh.exe`。它会读取 zsh / Oh My Zsh 配置意图，并在安全范围内
翻译为 winuxsh 原生能力。

## 为什么需要 Winuxsh

Windows 已经有 PowerShell，但很多用户和 agent 需要的是 bash/zsh-like 终端：
脚本语法尽量跨平台，命令行为接近 Unix，进程行为仍然是 Windows 原生。

Winuxsh 的产品契约是：

- **Windows 上的 Unix shell 语法**：通过 rubash 提供 bash-compatible 语义。
- **Windows 原生进程行为**：没有隔离文件系统根目录，没有 `/c` 路径权威，
  不让 PowerShell alias/wildcard 行为污染执行。
- **Zsh-like 交互体验**：autosuggestions、syntax highlighting、Vi/Emacs、
  Ctrl+R 历史搜索、多行 PS2 prompt、丰富 Tab 补全。
- **Agent 友好**：`winuxsh -c ...` 和脚本文件保持安静、确定性，并正确传播
  stdout/stderr/exit code。
- **安全 zsh 兼容**：扫描和翻译 `.zshrc` / Oh My Zsh 资产，不在启动时盲目
  source 任意 zsh 插件脚本。

## 当前状态

当前 `master` 是 rubash-backed rewrite，已经具备：

- 交互式 REPL 与非交互 `-c` / script 执行
- heredoc、continuation、多行 `if` / `for` / function、bash smoke fixture 的整体脚本执行
- Windows-native cwd/path 契约，默认显示 `C:/...`
- 常用 winuxcmd coreutils 内置补全
- 命令/路径补全，包括空 Tab 和 `gre<Tab>` 前缀命令补全
- Emacs/Vi 编辑模式、Ctrl+R 历史搜索、可配置 history/menu
- 原生 autosuggestions 和 syntax highlighting
- `~/.winuxsh/themes/*.toml` 用户主题
- 安全 `.zshrc` / Oh My Zsh scanner、report、import plan、显式 apply、status、rollback plan、doctor
- Git、Docker、kubectl、npm、autosuggestions、syntax highlighting、history widgets、direnv、dotenv、zoxide、thefuck、command-not-found、fzf、interactive-cd、last-working-dir 等原生 zsh-style packs

`--zsh-profile-plan zsh-lite` 这类 profile 命令还在规划中，当前请使用下面已经实现的兼容命令。

## 快速开始

从源码构建：

```pwsh
cargo build --release
```

启动交互式 shell：

```pwsh
target\release\winuxsh.exe
```

执行命令或脚本：

```pwsh
winuxsh -c "pwd; ls -la; git status"
winuxsh script.sh arg1 arg2
```

在 winuxsh 内：

```bash
pwd
ls -la
echo "home is $HOME"
for i in 1 2 3; do echo $i; done
if [ -f Cargo.toml ]; then echo ok; fi
cat Cargo.toml | grep name
```

原生 Windows 路径可直接作为 shell 输入：

```bash
ls C:/Users
ls C:\Users\YourName
cd C:/Users/YourName/repo
pwd
```

`pwd` 默认输出 Windows-native `C:/...`。旧的 `/c/...` 会作为兼容输入接受，但它不是默认路径风格，也不是 MSYS-style 假根目录。

## Zsh 用户从这里开始

如果你已有 `.zshrc`，不要把随机 zsh 插件源码复制进 winuxsh。先让 winuxsh 检查：

```pwsh
winuxsh --zsh-compat-report
winuxsh --zsh-compat-report-json
winuxsh --zsh-compat-import-plan
```

如果 import plan 看起来正确，再显式应用：

```pwsh
winuxsh --zsh-compat-import-apply
winuxsh --zsh-compat-import-status
winuxsh --zsh-compat-doctor
```

回滚也是先输出计划：

```pwsh
winuxsh --zsh-compat-import-rollback-plan
```

详细教程见：[Zsh 迁移教程](DOCS/zsh-migration-guide.md)。

## 原生 Zsh Packs

Winuxsh 内置常见 zsh / Oh My Zsh 插件行为的原生支持。这些能力由 winuxsh 自己通过 Rust、reedline、rubash 实现；不会 vendor zsh 插件源码，也不会在启动时 source 插件脚本。

查看当前 inventory：

```pwsh
winuxsh --zsh-native-packs
winuxsh --zsh-native-packs-json
```

默认启用的 pack 只限安全交互 UI：

- `zsh-autosuggestions`：基于历史的原生 inline suggestions
- `zsh-syntax-highlighting`：原生 main highlighter subset

有用但需要 opt-in 或未来 profile 启用的 pack 包括：

- `git`：Oh My Zsh-style aliases，如 `g`、`gst`、`gco`、`gl`、`gp`、`glog`
- `zsh-history-substring-search` 和标准 ZLE widget 映射
- `docker`、`kubectl`、`npm`
- `command-not-found`、`direnv`、`dotenv`、`zoxide`、`thefuck`、`fzf`、
  `zsh-interactive-cd`、`last-working-dir`

会读取项目文件、执行外部命令或改变 shell 状态的 lifecycle/external-command packs 默认保持关闭，必须在 `~/.winshrc.toml` 中显式启用。

## 配置

Winuxsh 使用 `~/.winshrc.toml` 作为原生控制面。`.zshrc` 是熟悉的导入来源，但运行时权威仍是 TOML，因为 TOML 显式、可回滚，也方便人和 agent 检查。

最小示例：

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
# 可选覆盖；省略时从 PATH 自动发现。
# path = "D:/tools/winuxcmd/winuxcmd.exe"
```

zsh compatibility import 只有在你显式运行 `--zsh-compat-import-apply` 时，才会把 managed block 写入同一个 TOML 文件。

## Shell 行为

Winuxsh 通过 rubash 执行 shell 语言，包括：

- 变量和命令替换
- 管道和重定向
- aliases 和 functions
- `if`、`for`、`while`、`case`、function、heredoc、continuation
- script files 和 `-c` 整体脚本执行

交互式多行输入会像普通 shell 一样等待完整块：

```bash
HTTP_CODE=200
if [ $HTTP_CODE -eq 200 ]; then
  echo OK
fi
```

REPL 会等待 `fi` 后一次性执行整个块。

## 补全系统

Winuxsh completion 支持：

- 内置 winuxcmd command definitions
- 从 Windows `PATH` / `PATHEXT` 补全命令
- 能处理空格路径 quote/escape 的路径补全
- 环境变量补全
- `completion_dirs` 中的用户 TOML definitions
- 可安全翻译的 zsh completion assets
- 显式 allowlist + timeout 的 dynamic/runtime completion providers

用户 TOML definitions 覆盖 built-ins。

## 架构

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

## 非目标

- 不做 PowerShell 语义或 wildcard 行为
- 不做 MSYS2/Git Bash/Cygwin/WSL 隔离
- 不采用 Nushell syntax 或 structured pipeline model
- 不在 winuxsh 内实现 zsh parser 或 ZLE runtime
- 不在启动时盲目 source `.zshrc` 或 zsh plugin scripts
- 不引入 winuxcmd FFI/DLL
- 不重复实现 rubash parser/executor/core shell semantics

## 文档

- [Zsh 迁移教程](DOCS/zsh-migration-guide.md)
- [Roadmap](DOCS/winuxsh-roadmap.md)
- [Architecture](DOCS/architecture.md)
- [Native Zsh Plugin Pack Plan](DOCS/winuxsh-native-zsh-plugin-pack-plan.md)
- [Zsh Compatibility Plan](DOCS/zsh-compatibility-plan.md)
- [Positioning and Feature Map](DOCS/winuxsh-positioning-and-feature-map.md)

## License

GPL-3.0-or-later，详见 [LICENSE](LICENSE)。