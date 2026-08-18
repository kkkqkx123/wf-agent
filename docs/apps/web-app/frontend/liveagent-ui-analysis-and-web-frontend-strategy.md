# LiveAgent-ui 功能分析及 Web 前端实现策略

## 概述

本文档基于对 `ref/LiveAgent-ui/` 参考实现和现有项目基础设施的分析，梳理 LiveAgent-ui 的功能架构、组件设计模式，并提出在当前项目（Modular Agent Framework）中实现 Web 前端的具体策略。

---

## 一、LiveAgent-ui 功能全景分析

### 1.1 技术栈

| 维度 | 选型 |
|------|------|
| 框架 | React 18 + TypeScript |
| 桌面壳 | Tauri（通过 `@tauri-apps/api` 调用 Rust 后端） |
| 样式 | TailwindCSS + CSS Modules |
| 状态管理 | React Hooks (useState/useReducer/useRef) + Context |
| 国际化 | 自建 i18n 模块（`LocaleContext` + 翻译函数） |
| 构建 | Vite（推测） |
| LLM SDK | `@earendil-works/pi-ai` |

### 1.2 功能模块总览

LiveAgent-ui 是一个桌面端 AI 编程助手，功能可划分为以下核心域：

#### 1.2.1 对话系统（核心功能）

**页面**：`pages/chat/` + `pages/ChatPage.tsx`

| 子模块 | 关键文件 | 功能描述 |
|--------|----------|----------|
| 对话转录（Transcript） | `transcript/` | 消息列表渲染、楼层导航、用户/助手气泡、加载态、空状态 |
| 对话气泡 | `components/assistant-bubble/` | 助手头像、工具调用展示、编辑 Diff、搜索结果、TODO 列表、用量面板 |
| 对话组合器（Composer） | `components/ChatComposerBar.tsx` | 输入框、提及（Mention）、附件、发送控制 |
| 运行时 | `runtime/` | `ChatRuntimeHost`、模型选择、对话回合发送、上下文构建 |
| Gateway | `gateway/` | Gateway 桥接、运行时状态快照、状态批处理 |
| 队列 | `queue/` | 对话回合队列管理 |
| 历史 | `history/` | 对话历史侧边栏、分支会话、共享历史 |
| 侧边栏 | `sidebar/` | 对话列表侧边栏容器 |

**对话状态管理**（`lib/chat/conversation/`）：
- `conversationState.ts` — 对话状态模型（消息、摘要、时间线渲染项）
- `liveTranscriptStore.ts` — 实时转录存储
- `chatAbort.ts` / `turnCancellation.ts` — 回合取消/中止

**轮次执行**（`lib/chat/runner/`）：
- `agentRunner.ts` — Agent 模式回合执行
- `deepSeekDsml.ts` — DeepSeek DSML 工具调用处理
- `seedToolCalls.ts` — 种子工具调用
- `toolCallArgumentGuard.ts` — 工具调用参数保护

#### 1.2.2 LLM 提供商集成

**目录**：`lib/providers/`

| 子模块 | 功能 |
|--------|------|
| `llm.ts` | 核心 LLM 接口导出 |
| `modelVendor.ts` | 模型供应商抽象 |
| `anthropicModels.ts` | Anthropic 模型特性 |
| `deepSeekProviderAdapter.ts` | DeepSeek 适配器 |
| `runtime/modelFactory.ts` | 模型工厂：根据配置创建模型实例 |
| `runtime/textOnlyRuntime.ts` | 纯文本模式流式运行时 |
| `runtime/streamByApi.ts` | 通用 API 流式调用 |
| `runtime/payloadPipeline.ts` | 请求载荷中间件流水线 |
| `runtime/requestOptions.ts` | 请求头、缓存策略构建 |
| `runtime/messageUtils.ts` | 消息工具函数 |
| `hostedSearchEvents.ts` | 托管搜索事件 |
| `customHeaders.ts` | 自定义请求头 |

支持的提供商类型：
- OpenAI 兼容（含 DeepSeek、xAI 等）
- Anthropic（Claude）
- Gemini
- Codex（自定义格式）

#### 1.2.3 工具系统

**目录**：`lib/tools/`

| 子模块 | 功能 |
|--------|------|
| `builtinRegistry.ts` | 内置工具注册中心 |
| `fsTools.ts` | 文件系统工具（读写文件、路径操作） |
| `shellTools.ts` | Shell 命令执行工具 |
| `terminalTools.ts` | 终端管理工具 |
| `memoryTools.ts` | 记忆读写工具 |
| `mcpTools.ts` | MCP 服务器调用工具 |
| `mcpManagerTools.ts` | MCP 服务器管理工具 |
| `skillTools.ts` | Skill 调用工具 |
| `todoTools.ts` | TODO 管理工具 |
| `cronTools.ts` | 定时任务工具 |
| `askUserQuestionTools.ts` | 用户提问（Human Relay）工具 |
| `sshManagerTools.ts` | SSH 隧道管理工具 |
| `tunnelManagerTools.ts` | 本地隧道管理工具 |
| `systemToolOptions.ts` | 系统工具选项（启用/禁用） |
| `customSystemTools.ts` | 自定义系统工具 |

#### 1.2.4 工作区 / 项目工具面板

**目录**：`components/project-tools/`

| 子模块 | 功能 |
|--------|------|
| **文件树** (`file-tree/`) | 项目文件浏览器、上下文菜单、行级渲染 |
| **Git 审查** (`git-review/`) | Diff 视图、状态视图、历史视图、工具栏 |
| **右侧面板** | 右侧停靠面板管理器 |
| `BackgroundTasksPanel.tsx` | 后台任务面板 |
| `LocalTunnelPanel.tsx` | 本地隧道面板 |
| `SshTunnelPanel.tsx` | SSH 隧道面板 |
| `XTermViewport.tsx` | XTerm 终端视图 |
| `RightDockPanel.tsx` | 右侧停靠面板容器 |
| `RightDockTabStrip.tsx` | 面板标签栏 |

**工作区编辑器**（`components/workspace-editor/`）：
- `WorkspaceCodeEditorOverlay.tsx` — 代码编辑器覆盖层
- `WorkspaceFilePreviewOverlay.tsx` — 文件预览覆盖层
- `WorkspaceImagePreviewOverlay.tsx` — 图片预览覆盖层
- `WorkspaceMarkdownPreview.tsx` — Markdown 预览
- `WorkspaceSftpPanel.tsx` — SFTP 文件传输面板
- `WorkspaceSshTerminalOverlay.tsx` — SSH 终端覆盖层

#### 1.2.5 Agent / 子代理系统

**目录**：`lib/subagents/`

| 文件 | 功能 |
|------|------|
| `protocol.ts` | 子代理通信协议 |
| `scheduler.ts` | 子代理调度器 |
| `run.ts` | 子代理运行逻辑 |
| `store.ts` | 子代理状态存储 |
| `roster.ts` | 子代理名册管理 |
| `card.ts` / `cards.ts` | 子代理 UI 卡片 |
| `agentTool.ts` | 子代理调用工具 |
| `sendMessageTool.ts` | 子代理间消息发送工具 |
| `ipc/` | 进程间通信（store + worktree） |

#### 1.2.6 记忆系统

**目录**：`lib/memory/`

| 子模块 | 功能 |
|--------|------|
| `api.ts` | 记忆 CRUD API（通过 Tauri invoke 调用 Rust MemoryStore） |
| `config.ts` | 记忆配置 |
| `schema.ts` | 记忆数据结构定义 |
| `extraction/` | 记忆提取（context, gating, planTool） |
| `organizer/` | 记忆整理（pipeline, quota, runRecord, service） |
| `prompts/` | 记忆相关 Prompt 模板 |

UI 组件：`pages/settings/memory/` — 记忆面板、记忆设置抽屉、整理历史

#### 1.2.7 Skill 系统

**目录**：`lib/skills/`

| 子模块 | 功能 |
|--------|------|
| `index.ts` | Skill 发现、摘要、加载 |
| `builtin.ts` | 内置 Skill 管理 |
| `clawHub.ts` / `clawHubCategories.ts` | Skill 市场 Hub |
| `stickCardMetadata.ts` | Skill 卡片元数据 |
| `skillTriggerHint.ts` | Skill 触发提示 |

UI 页面：`pages/skills-hub/SkillsHubPage.tsx` — Skill Hub 浏览页

#### 1.2.8 MCP 注册表

**目录**：`lib/mcpRegistry/`
- `index.ts` — MCP 服务器注册表管理

UI 页面：`pages/mcp-hub/` — MCP Hub 页、导入视图、注册表浏览器、服务器配置表单

#### 1.2.9 设置系统

**页面**：`pages/settings/`

| 子模块 | 功能 |
|--------|------|
| `SystemSettingsForm.tsx` | 系统设置（执行模式、工作目录、工作区项目） |
| `ProvidersSection.tsx` | LLM 提供商配置 |
| `AgentsSection.tsx` | Agent 配置 + Prompt 模板 |
| `SkillsSettingsForm.tsx` | Skill 设置 |
| `HooksSection.tsx` | 自动化钩子配置 |
| `CronSection.tsx` | 定时任务配置 |
| `SshSection.tsx` | SSH 连接配置 |
| `RemoteSection.tsx` | 远程连接配置 |
| `GlobalShortcutsSection.tsx` | 全局快捷键配置 |
| `SystemToolsSection.tsx` | 系统工具启用/禁用 |
| `AboutSection.tsx` | 关于 / 更新 |
| `memory/` | 记忆系统设置面板 |

#### 1.2.10 聊天相关组件

**目录**：`components/chat/`

| 组件 | 功能 |
|------|------|
| `ChatHistorySidebar.tsx` | 历史对话侧边栏 |
| `MentionComposer.tsx` | @提及组合器 |
| `AskUserQuestionCard.tsx` | 用户提问卡片 |
| `ChangedFilesCard.tsx` | 文件变更卡片 |
| `ComposerAttachmentCard.tsx` | 组合器附件卡片 |
| `FileChangeBadge.tsx` | 文件变更徽标 |
| `HistoryShareModal.tsx` | 历史分享模态框 |
| `SharedHistoryManagerModal.tsx` | 共享历史管理模态框 |

#### 1.2.11 自动化 / Hooks

**目录**：`lib/automation/`
- `index.ts` — 自动化初始化
- `backend.ts` — 自动化后端桥接
- `hookRunner.ts` — Hook 执行器
- `store.ts` — 自动化状态存储

#### 1.2.12 国际化

**目录**：`i18n/`
- `LocaleContext.tsx` — Locale Context Provider
- `config.ts` — 语言配置
- `index.ts` — i18n 导出

#### 1.2.13 Git 集成

**目录**：`lib/git/`
- `tauriGitClient.ts` — Tauri 本机 Git 客户端
- `gitGraph.ts` — Git 图谱渲染数据
- `types.ts` — Git 类型定义

Git 分支选择器组件：`components/git/GitBranchSelector.tsx`

#### 1.2.14 终端系统

**目录**：`lib/terminal/`
- `types.ts` — 终端会话类型定义
- `sessionStore.ts` — 终端会话状态存储
- `tauriTerminalClient.ts` — Tauri 本机终端客户端

#### 1.2.15 对话压缩（Compaction）

**目录**：`lib/chat/compaction/`
- 压缩引擎、策略、验证、Token 账本、文件账本、摘要 Prompt 等

#### 1.2.16 对话消息

**目录**：`lib/chat/messages/`
- `uiMessages.ts` — UI 消息构建
- `uploadedFiles.ts` — 上传文件管理
- `changedFiles.ts` / `fileChangeStats.ts` — 文件变更
- `mentionReferences.ts` — @提及引用
- `hostedSearch.ts` — 托管搜索结果
- `toolPreview.ts` — 工具调用预览

#### 1.2.17 其他

| 模块 | 功能 |
|------|------|
| `lib/chat-scroll/` | 滚动跟随引擎 |
| `lib/chat-floor-nav/` | 楼层导航（书签、模型） |
| `lib/reorder/` | 拖拽排序模型 |
| `lib/managed-process/` | 托管进程管理 |
| `lib/sftp/` | SFTP 客户端 |
| `lib/shortcuts/` | 全局快捷键 |
| `lib/models/` | 模型目录 |
| `lib/debug/` | 调试工具 |

---

## 二、LiveAgent-ui 架构模式分析

### 2.1 核心架构模式

```
App (根组件)
├── AppChrome (窗口壳)
├── CronPromptRunner (定时任务)
├── MemoryOrganizerHost (记忆整理)
├── ChatPage (对话页)
│   ├── ChatHeader (对话头)
│   ├── ChatTranscript (对话转录)
│   ├── ChatComposerBar (输入组合器)
│   ├── ChatSidebarContainer (会话侧边栏)
│   ├── ChatHistorySidebar (历史侧边栏)
│   └── RightDockPanel (右侧工具面板)
│       ├── FileTreePanel (文件树)
│       ├── GitReviewPanel (Git 审查)
│       ├── LocalTunnelPanel (隧道)
│       ├── SshTunnelPanel (SSH 隧道)
│       ├── BackgroundTasksPanel (后台任务)
│       └── XTermViewport (终端)
└── SettingsPage (设置页)
    ├── SystemSettingsForm
    ├── ProvidersSection
    ├── AgentsSection
    ├── CronSection
    ├── ...
```

### 2.2 关键设计模式

1. **Gateway 桥接模式** — 桌面端通过 Tauri invoke/listen 与 Rust 后端通信；Web 端通过 WebSocket 桥接
2. **Provider 适配器模式** — 各 LLM 提供商通过统一 `ProviderRuntimeConfig` 接口适配
3. **工具注册中心模式** — 所有内置工具通过 `BuiltinToolRegistry` 注册和调度
4. **事件驱动** — 全局事件总线（Tauri event system）驱动状态变更
5. **Context + Hooks 状态管理** — 无全局 Store 依赖，通过 React Context + Hooks 模式管理状态

### 2.3 与后端通信方式

当前 LiveAgent-ui 主要通过以下方式与后端通信：
- **Tauri invoke** — 直接调用 Rust 命令（文件操作、Git、终端、进程管理、记忆存储等）
- **Tauri event listen** — 监听后端事件（运行时状态更新、Gateway 同步等）

这意味着在 Web 端实现时，所有 `invoke` 调用需要替换为 **REST API / WebSocket** 调用。

---

## 三、当前项目基础设施分析

### 3.1 已有成果

| 组件 | 状态 | 说明 |
|------|------|------|
| `apps/web-app/` | 脚手架 | SvelteKit 项目，仅含一个空页面，无实质 UI 组件 |
| `apps/server/` | 较完整 | Express 后端，20+ 路由模块，适配器层，SSE/WS 支持 |
| `apps/cli-app/` | 较完整 | CLI 应用，可直接调用 SDK 执行工作流和 Agent Loop |
| `packages/sdk/` | 较完整 | 核心 SDK：工作流引擎、Agent Loop、Checkpoint、工具系统等 |
| `docs/apps/web-app/` | 设计文档 | 架构设计、实现阶段计划、功能清单 |

### 3.2 后端 API 覆盖度（server 端）

`apps/server/` 已实现的 API 路由：

| 路由模块 | 适配器 | 功能 |
|----------|--------|------|
| `workflows` | `workflow-adapter` | 工作流 CRUD + 注册 |
| `executions` | `execution-comparison-adapter` | 执行实例管理 |
| `events` | `event-adapter` | 事件查询、统计 |
| `versions` | `workflow-version-adapter` | 工作流版本管理 |
| `graphs` | `workflow-graph-adapter` | 工作流图谱 |
| `checkpoints` | `workflow-execution-checkpoint-adapter` | 检查点管理 |
| `tools` | `tool-adapter` | 工具注册表 |
| `templates` | `template-adapter` | 模板管理（Agent/工作流模板） |
| `scripts` | `script-adapter` | 脚本管理 |
| `variables` | `variable-adapter` | 变量管理 |
| `triggers` | `trigger-adapter` | 触发器管理 |
| `messages` | `message-adapter` | 消息管理 |
| `agent-loops` | `agent-loop-adapter` | Agent Loop 管理 |
| `iterations` | `iteration-analysis-adapter` | 迭代分析 |
| `agent-profiles` | `agent-profile-adapter` | Agent 配置 |
| `llm-profiles` | `llm-profile-adapter` | LLM 提供商配置 |
| `skills` | `skill-adapter` | Skill 管理 |
| `progress` | `progress-tracking-adapter` | 进度追踪 |
| `comparisons` | — | 执行对比 |
| `metrics` | `metrics-adapter` | 指标收集 |
| `search` | `search-adapter` | 搜索 |
| `storage` | `storage-diagnostics-adapter` | 存储诊断 |
| `sse` | — | SSE 实时推送端点 |
| `interactions` | `interaction-service` | Human Relay 交互 |

### 3.3 已有设计文档

已有 `docs/apps/web-app/` 目录中的文档：

| 文档 | 说明 |
|------|------|
| `architecture.md` | Web 应用架构设计（分层架构、数据流、路由） |
| `implementation-phase-1.md` ~ `phase-4.md` | 分阶段实现计划 |
| `web-app-feature-list.md` | 详细功能清单 |
| `svelte-best-practice.md` | Svelte 最佳实践 |
| `svelte-react-compare.md` | Svelte vs React 对比 |
| `streaming-svelte.md` | Svelte 流式渲染 |
| `websocket-necessity-analysis.md` | WebSocket 必要性分析 |

---

## 四、Web 前端实现策略

### 4.1 总体原则

1. **不照搬 LiveAgent-ui 代码** — 参考其架构和功能拆分，但按 Svelte/web 技术栈重新实现
2. **复用现有 server API** — 前端直接对接 `apps/server/` 提供的 REST + SSE/WebSocket API
3. **模块化渐进式实现** — 按功能域逐个模块实现，降低复杂度
4. **类型共享** — 通过 `packages/types` 或新建共享类型包，确保前后端类型一致
5. **桌面特有功能降级** — 终端、文件系统、本机 Git 等功能在 Web 端通过 Server API 提供

### 4.2 架构决策

#### 4.2.1 技术选型确认

| 决策 | 选择 | 理由 |
|------|------|------|
| 框架 | **Svelte 5**（已选，保持） | 已有脚手架，团队选型 |
| 状态管理 | **Svelte 5 Runes** + 模块级 Stores | 避免引入外部状态库 |
| 样式方案 | **TailwindCSS**（已选） | 一致性，与 LiveAgent-ui 一致 |
| 路由 | **SvelteKit**（已选） | 内置路由、SSR 可选 |
| UI 组件库 | **自建 + shadcn-svelte** | 保持风格统一，参考 LiveAgent-ui 的 UI 组件 |
| 实时通信 | **WebSocket**（ws 库） | 双向通信需求（Agent Loop 交互）> SSE 单向推送 |
| 可视化 | **D3.js 或 ECharts** | 工作流编辑器和执行流程图 |
| 代码编辑器 | **Monaco Editor**（CodeMirror 备选） | LiveAgent-ui 使用 Monaco |
| Markdown 渲染 | **svelte-markdown** 或自定义 | 对话消息渲染 |
| 测试 | **Vitest**（沿用项目配置） | 与 monorepo 统一 |

#### 4.2.2 前端架构层次

```
┌────────────────────────────────────────────────────┐
│                   Pages (SvelteKit routes)          │
│  /workflows  /threads  /agent-loops  /settings ... │
├────────────────────────────────────────────────────┤
│              Feature Components                     │
│  WorkflowEditor  ThreadMonitor  AgentChat  ToolMgr │
├────────────────────────────────────────────────────┤
│             Shared UI Components                    │
│  Button Modal Table Form Card Tooltip ScrollArea   │
├────────────────────────────────────────────────────┤
│            State Stores (Svelte Runes)              │
│  workflowStore  threadStore  agentLoopStore  ...    │
├────────────────────────────────────────────────────┤
│           API/Adapter Layer                         │
│  REST Client  WS Client  Event Bus                 │
├────────────────────────────────────────────────────┤
│                Backend (server)                     │
│  REST API  WebSocket  SSE                          │
└────────────────────────────────────────────────────┘
```

### 4.3 功能域与 LiveAgent-ui 映射

以下表格展示 LiveAgent-ui 功能模块到 Web 前端的映射策略：

#### 4.3.1 对话系统

| LiveAgent-ui | Web 前端策略 | 关键差异 |
|--------------|-------------|----------|
| `ChatPage` | `routes/agent-loops/[id]/chat` | 从桌面聊天 → Web Agent Loop 对话页面 |
| `ChatTranscript` | Svelte 实现 | 流式消息更新需适配 WebSocket |
| `ChatComposerBar` | Svelte 实现 | 无 Tauri 拖放文件，通过 HTML5 File API |
| `AssistantBubble` 系列 | Svelte 实现 | Markdown 渲染 + 工具调用卡片 |
| `liveTranscriptStore` | Svelte Store | 通过 WebSocket 事件更新 |
| `conversationState` | 前端 Store + 后端 API | 持久化由后端管理，前端只做缓存 |
| `chatTurnQueue` | 前端队列 | 使用 Svelte 响应式状态 |
| `agentRunner` | **后端执行** | Agent 轮次执行在 server 端，前端仅展示和下发用户消息 |

#### 4.3.2 LLM 提供商管理

| LiveAgent-ui | Web 前端策略 |
|--------------|-------------|
| Provider 配置页 | 通过 server API 管理，配置页面复用已有 `providers` 路由 |
| 模型选择/目录 | 通过 server API 获取可用模型列表 |
| LLM 流式运行时 | **不实现** — 由 server 端通过 SDK 调用，前端接收流式结果 |
| 模型工厂 | **不实现** — server 端管理 |

#### 4.3.3 工具系统

| LiveAgent-ui | Web 前端策略 |
|--------------|-------------|
| `builtinRegistry` | **不实现** — server 端管理工具注册 |
| `fsTools` | **不实现** — 通过 server API 操作文件 |
| `shellTools` | **不实现** — server 端执行 |
| `terminalTools` | WebSocket 代理到 server 终端管理 |
| `mcpTools` / `mcpManagerTools` | 通过 server API 管理 MCP |
| `todoTools` | 前端轻量组件 |
| `toolCallArgumentGuard` | server 端实现 |

#### 4.3.4 工作区 / 项目工具

| LiveAgent-ui | Web 前端策略 |
|--------------|-------------|
| 文件树 | Svelte 实现，通过 server API 获取文件列表 |
| Git 审查 | Svelte 实现，通过 server API（git 操作走 server） |
| Diff 视图 | Monaco Editor diff 编辑器 |
| XTerm 终端 | **降级** — Web 终端使用 xterm.js + WebSocket |
| SSH 隧道 | 通过 server API 管理 |
| 本地隧道 | **不支持** — 桌面特有功能 |
| SFTP | 通过 server API 文件上传/下载 |

#### 4.3.5 记忆系统

| LiveAgent-ui | Web 前端策略 |
|--------------|-------------|
| Memory API | 通过 server API（server 端对接 SDK MemoryStore） |
| 记忆设置 | 设置页面表单 |
| 记忆整理 | server 端调度 |
| 记忆面板 | Svelte 组件 |

#### 4.3.6 Skill 系统

| LiveAgent-ui | Web 前端策略 |
|--------------|-------------|
| Skill Hub 页 | Svelte 实现，通过 server API |
| Skill 设置 | 设置页面表单 |
| Skill 调用 | server 端管理 |

#### 4.3.7 设置页

LiveAgent-ui 的设置覆盖面很广，Web 端需重新组织：

| 设置分类 | Web 实现策略 |
|----------|-------------|
| 系统设置 | Svelte 表单，通过 server API 持久化 |
| LLM 提供商 | 复用 server `/api/llm-profiles` |
| Agent 配置 | 复用 server `/api/agent-profiles` |
| Skill 设置 | 复用 server `/api/skills` |
| 钩子/自动化 | 通过 server API 管理 |
| 定时任务 | 复用 server API |
| SSH 连接 | 通过 server API 管理 |
| MCP 服务器 | 复用 server `/api/tools` |
| 全局快捷键 | **Web 端不需要** |
| 记忆系统 | 通过 server API |

### 4.4 建议实现顺序

#### 阶段 1：基础框架 + 核心页面（P0）

1. **SvelteKit 项目完善** — 布局组件（Header/Sidebar/Content）、路由结构
2. **WebSocket 连接管理** — 建立 WS 连接 + 自动重连 + 心跳
3. **REST Client** — 封装 fetch API 调用
4. **工作流管理** — 列表页 + 详情页（使用 server `/api/workflows`）
5. **Agent Loop 交互** — 对话页（Agent Loop 列表 + 实时对话界面）
   - 这是 LiveAgent-ui 最核心的功能映射
   - 使用 WebSocket 接收流式消息

#### 阶段 2：监控 + 资源管理（P1）

6. **线程监控** — 执行列表 + 实时状态看板
7. **工具管理** — 工具注册表浏览
8. **LLM Profile 管理** — CRUD 配置页面
9. **Skill 管理** — Skill 列表 + 加载管理
10. **记忆系统** — 记忆查看 + 管理

#### 阶段 3：可视化 + 增强（P2）

11. **工作流可视化编辑器** — 拖拽节点 + 连线
12. **执行流程可视化** — 节点执行状态高亮
13. **事件监控** — 实时事件流
14. **检查点管理** — 创建/恢复/查看

#### 阶段 4：完善 + 优化（P3）

15. **MCP Hub**
16. **脚本管理**
17. **触发器管理**
18. **Human Relay**
19. **数据统计/指标**

### 4.5 关键实现建议

#### 4.5.1 Global State Store 结构

```
stores/
├── connection.ts      // WebSocket 连接状态
├── workflowStore.ts   // 工作流列表 + 当前工作流
├── threadStore.ts     // 线程列表 + 当前线程
├── agentLoopStore.ts  // Agent Loop 列表 + 当前会话
├── toolStore.ts       // 工具注册表
├── profileStore.ts    // LLM Profile
├── skillStore.ts      // Skill 列表
├── settingsStore.ts   // 应用设置
└── eventStore.ts      // 实时事件流
```

按 Svelte 5 Runes API 实现：

```typescript
// 示例：workflowStore.ts
import { writable } from 'svelte/store';
import { api } from '$lib/api';

function createWorkflowStore() {
  const workflows = writable<Workflow[]>([]);
  const current = writable<Workflow | null>(null);
  const loading = writable(false);

  async function fetchAll() {
    loading.set(true);
    try {
      const data = await api.get('/api/workflows');
      workflows.set(data);
    } finally {
      loading.set(false);
    }
  }

  return { workflows, current, loading, fetchAll, subscribe: workflows.subscribe };
}

export const workflowStore = createWorkflowStore();
```

#### 4.5.2 WebSocket 通信模式

```
客户端 → 服务端: JSON 消息
{
  "type": "subscribe" | "unsubscribe" | "execute" | "cancel",
  "payload": { ... }
}

服务端 → 客户端: JSON 事件
{
  "type": "event" | "result" | "error",
  "event": "agent-loop:started" | "agent-loop:text" | "thread:progress" | ...,
  "payload": { ... }
}
```

```typescript
// lib/ws.ts — WebSocket 管理
class WSClient {
  private ws: WebSocket | null = null;
  private handlers = new Map<string, Set<(data: unknown) => void>>();

  connect(url: string) {
    this.ws = new WebSocket(url);
    this.ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      const handlers = this.handlers.get(msg.event);
      handlers?.forEach(h => h(msg.payload));
    };
  }

  on(event: string, handler: (data: unknown) => void) {
    if (!this.handlers.has(event)) {
      this.handlers.set(event, new Set());
    }
    this.handlers.get(event)!.add(handler);
  }

  send(type: string, payload: unknown) {
    this.ws?.send(JSON.stringify({ type, payload }));
  }
}
```

#### 4.5.3 与 LiveAgent-ui 的核心差异

| 维度 | LiveAgent-ui (桌面) | Web 前端 |
|------|-------------------|---------|
| **执行层** | 前端直接调用 LLM API + 管理工具 | 前端 → Server → SDK → LLM + 工具 |
| **文件系统** | Tauri invoke 直接操作 | 通过 Server API 操作 |
| **终端** | Tauri 本机终端 | xterm.js + WebSocket |
| **Git** | Tauri 本机 Git | Server 端 Git 操作 |
| **流式消息** | 前端直接流式调用 LLM | WebSocket 接收 server 转发 |
| **离线能力** | 本地运行 | 需要后端服务 |
| **单/多用户** | 单用户 | 可支持多用户 |

#### 4.5.4 需要新建的后端 API（补充 server 端）

当前 server 端缺少的部分 API：

| API | 说明 |
|-----|------|
| 文件系统 API | 文件列表/读取/写入/上传/下载 |
| Terminal WebSocket | 终端会话管理 + WebSocket 代理 |
| Git Web API | Git 状态/Diff/历史/分支（通过 server 执行 git 命令） |
| WebSocket Agent Loop | 支持 Agent Loop 实时交互（替代 SSE） |
| 认证/会话管理 | 如果支持多用户 |

### 4.6 页面路由设计

```typescript
// SvelteKit 路由建议
routes/
├── +layout.svelte          // 主布局（侧边栏 + 顶部栏 + 内容区）
├── +page.svelte            // 首页（Dashboard / 工作流概览）
├── workflows/
│   ├── +page.svelte        // 工作流列表
│   ├── [id]/
│   │   ├── +page.svelte    // 工作流详情
│   │   ├── edit/+page.svelte // 工作流编辑
│   │   └── history/+page.svelte // 执行历史
├── threads/
│   ├── +page.svelte        // 线程列表
│   └── [id]/
│       └── +page.svelte    // 线程监控详情
├── agent-loops/
│   ├── +page.svelte        // Agent Loop 列表
│   └── [id]/
│       └── chat/+page.svelte // 实时对话界面
├── resources/
│   ├── tools/+page.svelte  // 工具管理
│   ├── profiles/+page.svelte // LLM Profile
│   ├── skills/+page.svelte // Skill 管理
│   ├── scripts/+page.svelte // 脚本管理
│   └── memory/+page.svelte // 记忆管理
├── settings/
│   ├── +page.svelte        // 设置（可侧边栏切换分类）
│   ├── mcp/+page.svelte    // MCP 配置
│   └── triggers/+page.svelte // 触发器
└── events/
    └── +page.svelte        // 事件监控
```

### 4.7 共享 UI 组件清单（参考 LiveAgent-ui）

LiveAgent-ui 在 `components/ui/` 中定义了基础 UI 组件，Web 端应实现类似的组件库：

| 组件 | 说明 |
|------|------|
| `Button` | 按钮 |
| `Input` | 输入框 |
| `Textarea` | 文本域 |
| `Select` | 选择器 |
| `Label` | 标签 |
| `ScrollArea` | 滚动区域 |
| `DropdownMenu` | 下拉菜单 |
| `ConfirmDialog` | 确认对话框 |
| `ConfirmActionPopover` | 确认操作弹出框 |

建议使用 **shadcn-svelte**（Svelte 版 shadcn/ui）来快速构建这些组件。

---

## 五、总结

### 5.1 LiveAgent-ui 功能总结

LiveAgent-ui 是一个功能完整的桌面端 AI 编程助手，覆盖了以下核心领域：

1. **AI 对话** — 完整的对话 UI + 流式消息 + 多模型支持
2. **工具系统** — 文件操作、Shell 执行、Git 操作、终端管理、MCP 集成
3. **工作区管理** — 项目文件浏览、Git 审查、代码编辑、文件预览
4. **记忆系统** — 自动记忆提取 + 定期整理
5. **Skill 系统** — Skill 发现、加载、调用
6. **自动化** — Hooks、定时任务
7. **系统管理** — 全面设置、LLM 提供商配置、快捷键

### 5.2 Web 前端实现策略核心结论

| 策略 | 说明 |
|------|------|
| **参考架构，非代码复用** | 借鉴 LiveAgent-ui 的模块划分和组件设计，用 Svelte 重写 |
| **前端轻量化** | 代理模式：前端只负责 UI 和交互，执行逻辑由 server 端承担 |
| **渐进式实现** | 按 P0 → P3 优先级逐步实现，先做 Agent Loop 对话和工作流管理 |
| **实时通信首选 WebSocket** | Agent Loop 交互需求双向通信，SSE 仅做辅助 |
| **桌面特有功能降级** | 文件系统、本机终端、Git 操作等通过 server API 间接实现 |
| **共享 UI 组件库** | 使用 shadcn-svelte 加速开发，保持风格统一 |
| **类型一致性** | 通过 monorepo 共享类型包确保前后端接口一致 |
