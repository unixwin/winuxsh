---
tags: [winuxsh, roadmap, v2]
created: 2026-07-13
status: active
---

# Winuxsh Roadmap

> Windows 原生、无隔离、给人和 agent 使用的 bash/zsh-like 终端
> 核心公式: winuxsh = rubash (shell 引擎) + winuxcmd.exe (coreutils) + reedline (REPL)

## 已完成 (v2 重写)

- [x] rewrite/v2-rubash 分支落地,单 commit c7e2c3c (+1514/-17817)
- [x] rubash 作为 lib 依赖,不再自实现 lexer/parser/ast/builtins
- [x] winuxcmd 通过 PATH 注入集成,不依赖 FFI/DLL
- [x] 补全系统 (TOML + bash 自动导入 + 三级缓存 + ListMenu)
- [x] 主题系统 (4 内置主题: default/dark/light/colorful)
- [x] Ctrl+C Win32 处理
- [x] REPL (reedline + 历史文件 .winuxsh_history)
- [x] 上游 PR unixwin/rubash#5 合入 (Windows PATH 大小写修复)
- [x] cargo build 零警告,14/14 测试通过
- [x] architecture.md、v2-plan.md 落盘 vault

## 短期 - v2.1

### CI 基础设施
- [x] .github/workflows/ci.yml - push/PR 自动 cargo build + cargo test (PR #9 合入)
- [x] Windows 平台 (当前仅支持 Windows)
- [x] cargo fmt --check lint 步骤

### 兼容性测试套件
- [x] tests/compat/ 目录, .sh + .expected 配对
- [x] 覆盖: 变量展开、命令替换、管道、if/for/case/function、别名、exit code、echo flags (heredoc 待 T-4)
- [x] 通过 cargo test --test compat -- --ignored 运行 (10 个 fixture)

### master 合并
- [x] PR #9: rewrite/v2-rubash -> master, 已 squash 合并到 master (commit a50638f)
        修复了 cargo fmt --check 与 Cargo.lock tracked 问题，CI 全绿

### 脚本执行改进
- [x] T-4: execute_script 整体 tokenize+parse+execute，支持 heredoc / continuation / 多行 if/for (commit 792416f)

## 中期 - v2.2

### 工作方式
- [x] 先做 Nushell / 现代 Windows shell reference audit，仅参考设计，不引入 Nushell 依赖，不 vendor 外部源码
- [ ] 每个功能阶段先更新 Markdown 计划，再小步实现、测试、提交（v2.2 实施中）
- [x] Obsidian vault 中维护 `winuxsh/` 文件夹作为项目长期记忆
- [x] Nushell reference audit 落盘: `DOCS/nushell-reference-audit.md`
- [x] zsh / Oh My Zsh / zsh 插件 reference audit 落盘: `DOCS/zsh-reference-audit.md`
- [x] zsh-first 功能定位与现代 shell reference map 落盘: `DOCS/winuxsh-positioning-and-feature-map.md`
- [x] Windows 原生 agent/user terminal 下一步计划落盘: `DOCS/winuxsh-next-development-plan.md`
- [x] zsh 配置与插件兼容计划落盘: `DOCS/zsh-compatibility-plan.md`
- [x] zsh 兼容接口可行性审计落盘: `DOCS/zsh-compatibility-interface-audit.md`
- [x] Phase 0 hygiene: 清理误建的空 `--help` 目录，保留 `.tmp/` 未跟踪

### 补全系统增强
- [x] Phase 1 baseline: 修复 completion integration test stale API (`load_completion_dirs`)
- [x] Phase 2 foundation: 内置 `ls` / `grep` / `find` 默认补全定义
- [x] Phase 3 expansion: 内置 `cat` / `cp` / `mv` / `rm` / `mkdir` / `touch` / `chmod` 默认补全定义
- [ ] 扩充默认 TOML 补全定义的命令覆盖范围
- [ ] bash 自动导入覆盖更复杂的 complete 调用模式
- [ ] 补全三级缓存的 TTL/失效策略

### 配置一致性
- [x] Phase 5 config: [winuxcmd].path 参与 PATH injection

### 用户体验
- [x] Vi 模式 (reedline 原生支持,主要工作量在键位配置)
- [x] Ctrl+R 历史搜索 (reedline 原生)
- [ ] 更多 prompt 自定义模板
- [x] Phase 6 themes: 用户自定义主题加载 (从 ~/.winuxsh/themes/)
- [x] zsh compat report CLI: 先输出扫描报告，不自动修改启动行为
- [x] zsh profile scanner/apply 第一层: `[zsh].auto_apply` 安全导入 `.zshrc` env/PATH/alias
- [x] Oh My Zsh layout importer Phase 2a: 静态 `_cmd` / `#compdef` / `_arguments` completion 资产翻译
- [x] zsh plugin tier importer Phase 2b: 插件分层报告 completion-only / alias-only / native-needed / unsupported
- [x] 原生 autosuggestions Phase 4a: 参考 zsh-autosuggestions，用 reedline history hinter 实现
- [x] 原生 syntax highlighting Phase 5a: 参考 zsh-syntax-highlighting main highlighter，用 reedline 实现
- [x] zsh prompt/theme compatibility Phase 6a: 扫描 `PROMPT` / `RPROMPT` 与简单 Oh My Zsh theme，翻译为 native prompt template
- [x] zsh Git prompt compatibility Phase 6b: 将 `$(git_prompt_info)` 桥接到 native `{git_prompt}` / `.git/HEAD` 渲染

## 长期 - v3

### 插件框架
- [x] v3 design doc opened: winuxsh-v3-plan.md
- [ ] 插件加载机制选型 (WASI / WASM / Rust dynlib)
- [ ] 插件 API 定义
- [ ] 插件市场

### Oh-My-Winuxsh
- [ ] 主题市场
- [ ] 插件管理 CLI
- [x] Phase 7a import-plan CLI: `--zsh-compat-import-plan` 输出可审阅 `.winshrc.toml` patch，不自动写用户配置
- [x] Phase 7b import-apply CLI: `--zsh-compat-import-apply` 显式写入 `.winshrc.toml`，写前备份，仅替换 winuxsh 管理块
- [x] Phase 7c import-status CLI: `--zsh-compat-import-status` 只读检查 managed block / TOML / 备份 / 下一次 apply 可行性
- [x] Phase 7d rollback-plan CLI: `--zsh-compat-import-rollback-plan` 只读输出最近备份与恢复命令
- [x] Phase 7e doctor CLI: `--zsh-compat-doctor` 聚合 scan/status/rollback，给出安全 apply 判断和下一步命令
- [x] Phase 8a native plugin pack: `plugins=(git)` 缺少 OMZ 插件目录时提供保守 git alias pack，不覆盖用户 alias
- [x] Phase 8b native plugin pack: `plugins=(docker)` 缺少 OMZ 插件目录时提供保守 docker alias pack，不覆盖用户 alias
- [x] Phase 8c dynamic completion scan: 识别 `tool completion zsh` 这类动态 completion generator，报告为 native provider 待接入
- [x] Phase 8d dynamic completion translation: 用注入 runner 将 `tool completion zsh` 输出翻译为 winuxsh `CommandDef`，尚不在启动时执行外部命令
- [x] Phase 8e dynamic completion runner: 显式 allowlist + timeout 执行动态 generator，默认不运行外部命令
- [ ] zsh/Oh My Zsh 兼容导入层（completion/theme/alias/native UX modules）

### Rubash 能力验证
- [ ] 梳理 rubash 已通过的 bash 上游测试能力矩阵
- [ ] 为 winuxsh host 层补充 PATH/env/cwd/home/stdout/stderr/exit-code 嵌入测试
- [ ] 作业控制/内建命令语义优先走 rubash，不在 winuxsh 重复实现

## 关键架构决策 (锁定)

- License: GPL-3.0-or-later (与 rubash 一致,同 unixwin org)
- rubash 集成方式: git 依赖,非本地路径
- winuxcmd 集成方式: PATH 注入,非 FFI/DLL
- 配置文件: .winshrc.toml (保留向后兼容)
- 历史文件: .winuxsh_history
- rust-version: 1.70 (minimum)
- rubash 锁定版本: f08d6d68e4901332c0003be549f9f80f6251ae2 (PR #5 合入点)
- 插件框架 + Oh-My-Winuxsh: v1 排除,plan for v2/v3

---

参见: architecture.md | v2-plan.md | rubash-pr-windows-path.md | winuxsh-v2.2-reference-plan.md | winuxsh-v3-plan.md | winuxsh-positioning-and-feature-map.md | winuxsh-next-development-plan.md | zsh-reference-audit.md | zsh-compatibility-plan.md | zsh-compatibility-interface-audit.md
