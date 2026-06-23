# 沙箱策略实现方案

| 策略文档 | 标识 | 语言 | 级别 | 跨平台 |
|---------|------|------|------|--------|
| [Shell 静态分析](shell-static-analyzer.md) | `static-analyzer` | Shell | 轻量 | ✅ |
| [Python 沙箱](python-sandbox.md) | `builtin-hook` / `ast-analyzer` / `pyodide-wasm` | Python | 轻量~重量 | ✅/✅/✅ |
| [JavaScript 沙箱](javascript-sandbox.md) | `vm-context` / `isolated-vm` | JavaScript | 轻量~中级 | ✅/部分 |
| [OS 级 Hook](os-level-hook.md) | `os-hook` | 全语言 | OS级 | ❌ (平台相关) |
| [VFS 覆盖层](vfs-overlay.md) | — | 全语言 | 文件系统 | ✅ |

## 选择指南

| 安全需求 | 推荐组合 |
|---------|---------|
| 基础命令行保护 | `shell: ["static-analyzer"]` |
| 全语言轻量保护 | `shell: ["static-analyzer"]`, `python: ["ast-analyzer"]`, `js: ["vm-context"]` |
| 高安全 Linux 环境 | 所有语言: `["os-hook", "static-analyzer"]` |
| 文件操作隔离 | 启用 VFS + 任意策略组合 |