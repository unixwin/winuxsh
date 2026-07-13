---
tags: [winuxsh, roadmap, v2]
created: 2026-07-13
status: active
---

# Winuxsh Roadmap

> 在 Windows 上还原 bash/zsh 使用体验的 Shell
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
- [ ] 从逐行 execute_line 切换到 execute_script_file 以支持 heredoc / 多行 continuation

## 中期 - v2.2

### 补全系统增强
- [ ] 扩充默认 TOML 补全定义的命令覆盖范围
- [ ] bash 自动导入覆盖更复杂的 complete 调用模式
- [ ] 补全三级缓存的 TTL/失效策略

### 用户体验
- [ ] Vi 模式 (reedline 原生支持,主要工作量在键位配置)
- [ ] Ctrl+R 历史搜索 (reedline 原生)
- [ ] 更多 prompt 自定义模板
- [ ] 用户自定义主题加载 (从 ~/.winuxsh/themes/)

## 长期 - v3

### 插件框架
- [ ] 插件加载机制选型 (WASI / WASM / Rust dynlib)
- [ ] 插件 API 定义
- [ ] 插件市场

### Oh-My-Winuxsh
- [ ] 主题市场
- [ ] 插件管理 CLI

### 作业控制
- [ ] 前台/后台任务管理
- [ ] jobs / fg / bg / kill 内建命令

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

参见: architecture.md | v2-plan.md | rubash-pr-windows-path.md
