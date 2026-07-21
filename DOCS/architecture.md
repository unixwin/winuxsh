# Winuxsh v2 Architecture

> 基于 rubash + winuxcmd 的 Windows 原生 bash/zsh-like terminal

## 项目定位

winuxsh 是一个 Windows 原生、无隔离、给人和 agent 都可以直接使用的 bash/zsh-like terminal。它**不自己实现 shell 语言**，而是作为 **rubash lib**（bash 兼容引擎）的交互式前端 + **winuxcmd**（coreutils）的路由层。它的核心价值在于 Windows 原生进程/环境体验：reedline REPL、补全系统、主题系统、Ctrl+C 处理、终端集成，以及稳定的非交互式 agent 执行契约。

winuxsh 不是 MSYS2、Git Bash、Cygwin 或 WSL 风格的隔离环境。`~` 指向普通 Windows 用户 home（PowerShell 中的 home / `USERPROFILE` / `dirs::home_dir()`），PATH、cwd、env、stdout、stderr、exit code 都是正常 Windows 进程状态。

## 三层架构

```
winuxsh.exe
├── winuxsh 自身层 (Rust)
│   ├── rubash::Executor         ← shell 语言引擎 (lexer/parser/execution/builtins)
│   ├── reedline REPL            ← 行编辑、历史、补全
│   ├── completion/              ← TOML + bash 自动导入 + 三级缓存
│   ├── theme/                   ← 主题与 prompt 模板
│   ├── config                   ← .winshrc.toml 解析
│   └── ctrl_c                   ← Win32 Ctrl+C 处理
├── rubash lib (Rust)
│   ├── lexer/parser/ast
│   ├── executor (pipeline/redirect/alias/function/array/job)
│   └── builtins (cd/source/export/set/test/printf...)
└── winuxcmd.exe (C++)           ← Unix coreutils (ls/cat/grep/find/cp/mv/rm...)
```

## 关键设计决策

### 1. rubash 作为 lib 依赖

winuxsh 直接链接 rubash 作为 Rust crate 依赖：

```toml
[dependencies]
rubash = { git = "https://github.com/unixwin/rubash.git", branch = "master" }
```

所有 shell 语义（解析、执行、内建命令、变量展开、重定向、管道、作业控制）委托给 rubash。winuxsh 不重复实现 lexer/parser/ast/builtins。

### 2. winuxcmd 通过 PATH 注入集成

不是通过 FFI/DLL——rubash Executor 内部通过 `find_user_command()` 在 PATH 查找外部命令。winuxsh 在启动时：

1. 探测 `winuxcmd.exe` 位置（优先 exe 同目录、其次 PATH）
2. 将其所在目录前置到进程 `PATH` 环境变量
3. rubash 的 PATH 查找自然先命中 winuxcmd 提供的 `ls`/`cat`/`grep` 等命令

### 3. 补全系统独立于引擎

补全系统（TOML 定义 + bash 脚本自动导入 + `cmd -h` 描述抓取 + 三级缓存）在 winuxsh 侧实现，不依赖 rubash。这是 winuxsh 的核心差异化能力。

### 4. 配置驱动

- 启动时读取 `~/.winshrc.toml`
- 通过 `[aliases]` 写入 rubash 的 alias 表
- 通过 `[completions.completion_dirs]` 配置补全定义目录
- 通过 `[theme]` 配置主题
- 通过 `[winuxcmd]` 配置 winuxcmd 路径

## 目录结构

```
winuxsh/
├── Cargo.toml
├── LICENSE                   # GPL-3.0-or-later
├── README.md / README-zh.md
├── .winshrc.toml
├── crates/
│   └── winuxsh-runtime/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs        # 库入口
│           ├── shell.rs      # Shell 状态
│           ├── repl.rs       # reedline REPL
│           ├── ctrl_c.rs     # Win32 Ctrl+C
│           ├── config.rs     # 配置解析
│           ├── winuxcmd.rs   # winuxcmd 探测
│           ├── prompt.rs     # Prompt 渲染
│           ├── theme.rs      # 主题系统
│           └── completion/   # 补全系统
├── src/
│   └── main.rs               # 入口
└── DOCS/
    ├── architecture.md
    └── completion.md
```

## 数据流

```
用户输入 "ls -la | grep foo"
         │
         ▼
   reedline (行编辑 + 补全)
         │
         ▼
   shell.execute_line(line)      ← winuxsh-runtime
         │
         ├─ rubash::lexer::tokenize(line)
         ├─ rubash::parser::parse(tokens) → Ast
         └─ executor.execute_ast(&ast)    ← rubash 处理全部语义
                │
                ├─ 内建命令 (cd/source/echo...)
                ├─ 外部命令 → find_user_command("ls")
                │                   │ (PATH 已注入 winuxcmd 目录)
                │                   ▼
                │              winuxcmd.exe ls -la
                │
                ├─ 管道: | grep foo → find_user_command("grep")
                └─ 输出到 stdout
```

## 与旧架构的差异

| 方面 | v1 (旧 winuxsh) | v2 (新 winuxsh) |
|------|-----------------|-----------------|
| Shell 引擎 | 自研 winsh-lexer/parser/ast | rubash lib |
| Coreutils | winuxcmd FFI (DLL, 已禁用) | winuxcmd.exe 进程 (PATH 注入) |
| 命令路由 | command_router.rs 分类表 | rubash 内部 find_user_command |
| 内建命令 | builtins.rs 自实现 | rubash::builtins |
| 补全系统 | src/completion/ | 完整保留迁移 |
| 主题系统 | theme.rs (8 主题) | 精简为 4 内置主题 |
| 插件系统 | Plugin trait + Oh-My-Winuxsh | 移出 v1，后续迭代 |
| 许可协议 | MIT | GPL-3.0-or-later |

## 版本规划

- v2.2: rubash rewrite 稳定化、补全增强、Vi/Ctrl+R、配置一致性、用户主题
- v2.3: Windows 原生 terminal contract、agent 友好的非交互式行为、history/prompt/completion UX
- v2.4: zsh-like 交互体验 polish（右 prompt、提示、补全菜单、默认配置）
- v3: 插件/Oh-My-Winuxsh/package layer；shell 语义与作业控制仍优先由 rubash 提供
- 非目标: Linux/macOS 原生 shell 产品；rubash 可跨平台复用，但 winuxsh 产品目标是 Windows

---

*Last updated: 2026-07-17*
