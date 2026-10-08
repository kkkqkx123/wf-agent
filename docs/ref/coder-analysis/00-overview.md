# code-server 前后端 Git 功能实现分析 · 总览

> 配套文档：本分析对接 `docs/plan/frontend/*`（web 端 Git 工作台方案），聚焦"后端集成方式"与"VSCode 风格 git 前端操作"两份可借鉴点。

## 0. 文档目的与范围

应要求拉取 `https://github.com/coder/code-server`，分析其前后端 Git 功能的实现方式，提炼**前后端分别可借鉴的设计思想**，重点在两点：

1. **后端集成方式**——code-server 如何把 VSCode（及其 git 能力）接入自己的服务端；
2. **VSCode 风格的 git 前端操作**——其 git 扩展在浏览器里暴露了哪些交互范式。

本文是**分析与说明**，不是实现方案；可落地决策收敛在 `03-borrowable-design.md`，并回链到既有 `frontend/*` 方案。

## 1. 项目定位与关键事实（已核查）

| 维度 | 结论 |
|---|---|
| code-server 是什么 | VSCode 的服务端分支。把**完整 VSCode**跑在服务器上，浏览器作瘦客户端，渲染画面经 WebSocket 回传 |
| git 能力来自哪里 | **100% 来自 VSCode 内置 git 扩展**，code-server 自身不实现一行 git 逻辑 |
| vendored 子模块 | `lib/vscode` 是 git submodule，指向 `microsoft/vscode`（`.gitmodules`：`path = lib/vscode, url = https://github.com/microsoft/vscode`）；本次用 sparse-checkout 只拉取 `extensions/git` |

| 仓库 / 子树 | commit | 角色 |
|---|---|---|
| `coder/code-server` | `fa7caee4dd82df8ef78ba4c3b23a7f84c7c70370` | 服务端外壳 + 鉴权 + 反向代理 |
| `microsoft/vscode` @ `lib/vscode` | `04c0d99f4fb0d8afe6ce4f0c58e31e183ac3e4b1` | 完整 VSCode + git 扩展（vendored 子模块） |
| `extensions/git/src`（git 扩展源码） | 同上游 | 分析对象：前端操作 + 后端 git 驱动 |

> 说明：code-server 直连 GitHub 在本环境被 TLS 阻断，经 `gh-proxy.com` 镜像拉取；VSCode 子模块用 `--filter=blob:none` 部分克隆 + sparse-checkout 只取 `extensions/git`，避免整库 GB 级下载。

## 2. 分层结论（TL;DR）

- **后端集成分两层，都值得借鉴：**
  - **A. code-server ↔ VSCode 服务端**：薄反向代理 + 鉴权包裹，把 VSCode 当库加载并**委托全部流量**，不重写逻辑。
  - **B. git 扩展 ↔ git 可执行文件**：子进程**编排 `git` CLI** + **环境变量注入辅助脚本** + **带外 IPC 通道**处理交互式 git（凭据、编辑器、SSH）。
- **前端**：VSCode 的 SCM 面板 = 统一收件箱（4 个资源分组）+ 顶栏 commit 编辑器 + 状态栏分支/同步 + 命令面板兜底。比 lazygit 更结构化，**hunk/selection 级暂存**是最大亮点。

## 3. 最关键的一条

VSCode 解决"git 进程跑在服务器端、交互输入却在浏览器端"的范式——**`GIT_ASKPASS` / `GIT_EDITOR` 助手脚本 + 文件管道 + IPC 回传**——直接回应了既有 `frontend/03` 方案里 `P5` 这个"send_input 依赖常驻 shell"的阻塞型未知项。详见 `01-backend-integration.md` §2.2 与 `03-borrowable-design.md` §1.2。

## 4. 文档导航

| 文档 | 内容 |
|---|---|
| `01-backend-integration.md` | 后端集成方式（**核心**）：委托边界 + CLI 编排 + 带外交互通道 + 操作串行化 |
| `02-vscode-git-frontend.md` | VSCode 风格 git 前端操作：四分组 SCM、commit composer、状态栏、命令面板、冲突解决 |
| `03-borrowable-design.md` | 对 wf-agent 的可借鉴设计：映射既有 `frontend/*` 方案 + 决策 + 更新开放点 |
