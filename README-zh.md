# Winuxsh

中文 | [English](README.md)

一个面向 Windows 的 bash 兼容 shell，基于 [rubash](https://github.com/unixwin/rubash) + [winuxcmd](https://github.com/unixwin/WinuxCmd) 构建。

Winuxsh 不自己实现 shell 语言，而是作为 rubash（bash 兼容引擎）的 Windows 交互式前端，并通过 `PATH` 注入把 Unix 工具调用路由到 `winuxcmd.exe`。Winuxsh 自身负责体验层：reedline REPL、补全系统、主题、配置、Ctrl+C 处理、Windows 集成。

## 架构

```
winuxsh.exe
├── winuxsh runtime (Rust)
│   ├── rubash::Executor   ← shell 语言引擎 (lexer/parser/execution/builtins)
│   ├── reedline REPL      ← 行编辑、历史、补全弹窗
│   ├── 补全系统           ← TOML + bash 自动导入 + 磁盘缓存
│   ├── 主题/Prompt        ← 4 内置主题，模板化提示符
│   ├── 配置               ← ~/.winshrc.toml
│   └── ctrl_c             ← Win32 Ctrl+C 处理
├── rubash lib
│   ├── lexer/parser/ast
│   ├── executor (pipeline/redirect/alias/function/array)
│   └── builtins (cd/source/export/set/test/printf/...)
└── winuxcmd.exe           ← Unix coreutils (ls/cat/grep/find/cp/mv/rm/...)
```

启动时把 `winuxcmd.exe` 所在目录前置到 `PATH`，由 rubash 已有的 `find_user_command` 优先命中。

## 构建

前置条件：

- Rust 1.70+
- `winuxcmd.exe` 可从 `PATH` 找到（或通过 `WINUXCMD_PATH` 环境变量覆盖，或与 `winuxsh.exe` 同目录的 `./winuxcmd/` 子目录）

```pwsh
cargo build --release
# 二进制位于 target/release/winuxsh.exe
```

## 使用

```
winuxsh                       # 交互式 REPL
winuxsh -c "command"          # 执行单条命令并退出
winuxsh script.sh [args...]   # 运行脚本文件
winuxsh -h | --help
winuxsh -V | --version
```

shell 内通过 rubash 提供完整的 bash 语义，配合 winuxsh 自身的补全系统：

```bash
echo $PATH                       # 变量展开
ls -la                           # 通过 winuxcmd (PATH 注入)
echo "today: $(date +%Y)"        # 命令替换
for i in 1 2 3; do echo $i; done # 控制流
cat Cargo.toml | grep name       # 内建 + 外部管道
```

## 配置

`~/.winshrc.toml`:

```toml
[shell]
prompt_format = "{user}@{host} {cwd} {symbol}"

[theme]
current_theme = "default"      # default | dark | light | colorful

[editor]
edit_mode = "emacs"          # emacs | vi

[aliases]
ll = "ls -la"
la = "ls -a"

[completions]
# 包含 <cmd>.toml 或 <cmd>.bash 补全脚本的目录
completion_dirs = [
    "D:/shellTools/ripgrep/complete",
    "D:/shellTools/fd/autocomplete",
]

[winuxcmd]
# 可选覆盖；不配置则从 PATH 自动探测
# path = "D:/tools/winuxcmd/winuxcmd.exe"
```

## 补全系统

继承自 MVP6：

- **TOML 定义** — 把 `<cmd>.toml` 放进任意 `completion_dirs`
- **Bash 自动导入** — 解析 `_cmd.bash` / `cmd.bash` 中的 `opts="..."`
- **自动描述抓取** — 对每个缺描述的命令运行一次 `cmd -h`，结果落盘缓存
- **三级缓存** — 内存 → `.parsed.toml` → 子进程
- **ListMenu 弹窗** — 对齐的描述，`cmd -<Tab>`、文件路径、环境变量都适配

## 状态

当前分支 `rewrite/v2-rubash` 是 v1：核心 REPL + 补全 + 主题 + 配置。Vi 模式和 `Ctrl+R` 历史搜索已在 v2.2 可用；插件框架和 Oh-My-Winuxsh 将在后续迭代中实现。

## 许可

GPL-3.0-or-later，详见 [LICENSE](LICENSE)。
