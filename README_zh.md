# wf-agent — 模块化 Agent 框架

一个用 Rust 编写的模块化框架，在同一套运行时上统一图结构的工作流编排与自主 LLM Agent 循环。

[English Documentation](README.md)

`wf-agent` 让两种执行模型共享同一套运行时。团队可以把确定性强、可审查的流水线描述为工作流图，把开放式推理交给对话式 Agent 循环，并在两者之间复用工具、检查点、沙箱与存储能力。

---

## 概述

框架围绕两个对等的执行引擎构建：

- **工作流引擎（`wf-workflow`）**——执行由带类型节点组成的静态有向无环图（DAG）。执行路径在事前定义，因此流水线可检查、可复现、易于校验。
- **Agent 引擎（`wf-agent`）**——执行对话式 LLM Agent 循环。每一轮调用模型、处理响应（最终答案或工具调用）并重复，直到满足终止条件。调用图在运行时由模型决策产生。

两个引擎都构建在共享执行基础设施（`wf-execution-shared`）之上，该层提供事件、钩子、检查点、中断、重试、超时、执行层级与变量作用域。应用通过门面层（`wf-api`）、HTTP 传输层（`wf-server`）、CLI/TUI 前端，以及 Web 与 IDE 客户端来使用这些引擎。

---

## 核心概念

### 工作流执行

工作流是由边连接的节点图。节点带有类型（`START`、`END`、`LLM`、`CUSTOM` 等）与配置；边负责控制流路由并在节点间映射变量。执行前，静态定义会编译为运行时图，并针对结构、可达性与环进行校验。

运行时负责：

- 节点级与工作流级超时
- 带可配置退避的重试策略
- 失败策略（`retry`、`continue`、`fail`）
- 在可配置边界创建检查点
- 通过中断处理实现暂停、恢复与取消

工作流定义以 TOML 存储，支持变量、元数据、标签、版本管理以及导出/导入。

### Agent 循环执行

Agent 循环是对话式、迭代式的。引擎初始化消息历史、调用配置的 LLM，并由模型在给出最终答案与调用工具之间选择。工具结果会追加到会话中，循环随之继续。

可配置项包括：

- 最大迭代次数、整体执行时间预算与最大暂停时长
- 重试、单次调用超时，以及带告警阈值的 token 上限
- 流式响应与工具调用协议（原生或文本）
- 可用、初始加载、可发现与隐藏的工具集合
- 检查点节奏（按迭代、出错时、工具调用时）与内容选择
- 在迭代、LLM 调用、工具调用边界触发的钩子
- 违规策略与动态上下文注入

### 共享执行模型

两个引擎都遵循协调器-执行器-处理器（coordinator–executor–handler）模式：

```
引擎生命周期协调器
  └─ 执行器（无状态，单次执行入口）
       └─ 执行协调器（主循环）
            └─ 单元协调器（逐节点 / 逐迭代）
                 ├─ 钩子（前置 / 后置边界）
                 ├─ LLM 执行协调器（共享）
                 ├─ 工具执行协调器（共享）
                 └─ 目标检查与下一步路由
```

这种结构把生命周期管理、编排与单一职责的处理器彼此分离，并让两个引擎能够对接同一套共享服务。

---

## 功能特性

- **双执行引擎**——确定性的 DAG 工作流与自主 Agent 循环共享同一运行时。
- **检查点与恢复**——对执行状态与文件历史创建快照，并可从任意记录点恢复或继续。
- **Layertwine 文件历史**——面向多 Agent 协同编辑与人工审批的分层、可溯源编辑历史，追踪版本控制本身无法覆盖的未提交改动。
- **插件系统**——统一的插件 trait、清单与贡献注册表，支持三种执行后端：WebAssembly（强隔离、燃料计量、epoch 中断、WASI 授权）、Lua（进程内、受限标准库）与 Native 动态库（完全受信的第一方扩展）。
- **工具注册表与 MCP**——内置与自定义工具（文件系统、Shell、补丁、搜索、技能）的注册表，并通过子进程与流式 HTTP 传输支持 Model Context Protocol 客户端。
- **技能系统**——可发现、可复用的能力包，支持启用/禁用状态、内容获取、资源访问以及提示/查询端点。
- **脚本引擎**——面向多语言脚本的解析器与执行引擎，包含风险分级、模板与流程控制。
- **沙箱**——针对 Shell、Python、JavaScript 与 Lua 的 Policy/Strategy/Runtime 分层，将静态分析门禁与真实强制手段（seccomp BPF 过滤、包裹式运行时、内嵌 Lua VM）结合。Overlay VFS 提供写时复制的路径策略。
- **LLM 层**——与提供商无关的客户端，支持 OpenAI、Anthropic、Gemini 线协议、OpenAI 兼容的提供商定义、配置档、token 计数、流式生成，并可通过插件贡献提供商。
- **存储**——基于 SQLite、PostgreSQL 或内存后端的实体持久化，支持 JSON 元数据、过滤查询、批量写入、跨实体原子写、缓存与埋点。
- **触发器、钩子与事件**——事件驱动协调，用于调度、生命周期扩展与可观测性。
- **人工介入审批**——工具审批与用户交互记录，支持可配置策略与超时。
- **可观测性**——指标采集与导出（含 Prometheus）、执行审计追踪、错误根因分析、性能剖析与跨资源检索。
- **多种前端**——无头 CLI、轻量 TUI、完整 ratatui TUI、HTTP/SSE/WebSocket 服务、SvelteKit Web 应用与 VS Code 扩展。

---

## 架构

### 分层 crate

工作区划分为四层。每一层只依赖其下层，所有 crate 构成严格的有向无环图。

| 层 | Crate | 职责 |
|-------|--------|----------------|
| **Foundation** | `wf-types`、`wf-common`、`wf-core`、`wf-plugin-sdk` | 共享数据类型、通用工具、核心契约（事件、状态、层级、中断、条件）、插件 SDK 契约 |
| **Infra** | `wf-metrics`、`wf-config`、`wf-storage`、`wf-llm`、`wf-script`、`wf-sandbox`、`wf-shell`、`wf-plugin` | 横切服务：指标、配置、持久化、LLM 客户端、脚本引擎、沙箱、Shell/PTY、插件运行时 |
| **Checkpoint** | `checkpoint-base`、`checkpoint-state`、`checkpoint-file`、`wf-checkpoint`、`layertwine` | 执行快照、文件编辑历史、存储引擎，以及集成门面 |
| **Engine** | `wf-tools`、`wf-resource`、`wf-execution-shared`、`wf-agent`、`wf-workflow` | 工具执行与 MCP、资源注册表与渲染、共享执行基础设施、Agent 循环引擎、工作流引擎 |
| **App** | `wf-api`、`wf-server`、`wf-runtime`、`debugger`、CLI crate、TUI crate | 应用门面、HTTP 传输、运行时引导、调试与用户界面 |

App 层把前端归入 `cli/`，把底层终端构建块归入 `tui/`，使完整 TUI（`wf-tui`）与轻量 TUI（`wf-mini`）能够复用同一套组件。

### 依赖关系图

```
foundation:  wf-types ← wf-common ← wf-core
                                        wf-plugin-sdk

infra:       wf-metrics  wf-config  wf-storage  wf-llm
             wf-script   wf-sandbox wf-shell    wf-plugin
             checkpoint: checkpoint-base ← checkpoint-state
                         checkpoint-base ← checkpoint-file
                         wf-checkpoint   → checkpoint-base / checkpoint-state / checkpoint-file
                         layertwine      （存储引擎）

engine:      wf-tools ← wf-resource ← wf-execution-shared ← wf-agent ← wf-workflow

app:         wf-api  wf-server  wf-runtime

cli:         wf-cli-shared ← wf-headless / wf-mini / wf-tui ← wf-cli-demo
tui:         tui-clock ← tui-style ← tui-markdown
             tui-terminal → wf-cli-shared
             tui-core → tui-clock / tui-style / tui-markdown / tui-terminal / wf-api
             tui-components → tui-core / tui-style / tui-markdown
             tui-render → tui-components / tui-core / tui-style
             tui-debug → tui-core
```

### 检查点子系统

检查点子系统分离三项关注点：

- **`checkpoint-base`**——错误、actor、策略与增量。
- **`checkpoint-state`**——执行快照与恢复。
- **`checkpoint-file`**——文件历史、观测与分支。
- **`layertwine`**——基于 SQLite 的内嵌内容寻址存储引擎，包含 zstd 压缩、快照、增量、检查点与分支。

`wf-checkpoint` 是唯一向上层暴露的门面，负责归属、采样、合并策略与垃圾回收。只有可变状态会被序列化；不可变配置在恢复时重新提供。

### 插件系统

`wf-plugin` 提供统一的插件 trait、统一的清单格式与统一的贡献命名空间（节点类型、工具类型、LLM 编解码、事件处理器、中间件），与底层后端无关。引擎无需感知插件由哪种后端执行。

隔离强度按后端分层：

| 后端 | 隔离强度 | 适用场景 |
|---------|-----------|--------------|
| WebAssembly | 强——燃料预算、epoch 中断、内存与模块体积上限、WASI 授权默认拒绝 | 不可信的第三方扩展 |
| Lua | 受限——受限标准库、解释器时间与内存钩子、进程内 | 受信的轻量脚本 |
| Native | 无——动态库直接载入宿主进程 | 完全受信的第一方扩展 |

Wasm 同时支持核心模块与组件模型，可使用 Rust、Go 或 Python 编写。

### 沙箱

沙箱采用 Policy → Strategy 链 → Runtime 的分层结构。分析策略作为门禁先于任何执行运行，执行策略负责真正的运行。每种语言都有默认策略链：

| 语言 | 默认链 | 强制手段 |
|----------|---------------|------------|
| Shell | 静态分析器、VFS 门禁、OS 钩子 | 命令与路径分析，外加 seccomp BPF 与 rlimit |
| Python | AST 分析器、内置函数钩子 | AST 门禁，以及生成包裹代码中的 import/open/eval 限制 |
| JavaScript | VM 上下文 | 包裹式执行，模块经代理，禁用动态求值 |
| Lua | 静态分析器、内嵌 VM | token 级分析，内嵌 VM 中的 API 级隔离 |

未知策略与缺失门禁按 fail-closed 处理。严格模式拒绝违规，宽松模式记录违规后继续。每个决策都会产生审计事件。

### 应用层

- **`wf-api`**——面向应用的 API 门面。它持有组合根（`ApiContext`），包含引擎句柄、存储、事件总线、指标、LLM 网关、工具注册表、沙箱与实时注册表，并对外暴露供所有前端使用的函数式 API。
- **`wf-server`**——纯 HTTP 传输层（axum），把请求映射到 `wf-api` 调用。它提供带版本的 REST 接口、用于执行与事件流的 Server-Sent Events、WebSocket 订阅端点、Prometheus 指标端点，以及日志、CORS、API Key 认证与限流中间件。
- **`wf-runtime`**——引导运行时：注册表、存储、LLM、沙箱与 MCP 的装配。
- **CLI 与 TUI**——`wf-headless` 面向脚本与自动化，`wf-mini` 提供轻量终端界面，`wf-tui` 提供完整 ratatui 界面。执行跟踪统一覆盖工作流与 Agent 执行，并支持阻塞、前台与后台三种模式。
- **Web 应用**——SvelteKit 前端，用于工作流管理、执行监控、Agent 交互、资源管理与事件流。
- **VS Code 扩展**——为 Agent 执行提供动态编辑器上下文（当前编辑器、打开的标签页、诊断信息）。

---

## 仓库结构

```
.
├── crates/
│   ├── foundation/          # 类型、通用工具、核心契约、插件 SDK
│   ├── infra/               # 指标、配置、存储、LLM、脚本、沙箱、Shell、插件
│   │   └── checkpoint/      # 检查点子系统（状态、文件历史、存储引擎）
│   ├── engine/              # 工具、资源、共享执行、Agent、工作流
│   └── app/
│       ├── wf-api/          # 应用门面
│       ├── wf-server/       # HTTP 传输层
│       ├── wf-runtime/      # 运行时引导
│       ├── debugger/        # 执行调试
│       ├── cli/             # 无头、轻量、完整 TUI、共享逻辑、示例
│       └── tui/             # 底层终端构建块
├── apps/
│   ├── web-app/             # SvelteKit Web 前端
│   └── vscode-app/          # VS Code 扩展
├── configs/                 # 工作流、Agent 循环、模板、LLM 配置档、MCP、技能、服务端
├── docs/                    # 架构、API、CLI、参考与设计文档
└── .wf/                     # 项目级配置覆盖
```

---

## 配置

配置采用声明式与分层方式。项目级文件位于 `.wf/` 与 `configs/` 目录，覆盖：

- **工作流**——包含节点、边、变量、重试与检查点策略的图定义。
- **Agent 循环**——模型配置档、提示词、工具暴露、检查点节奏、钩子。
- **节点与提示词模板**——可复用的构建块。
- **LLM 提供商与配置档**——提供商线协议与逐模型配置档。
- **MCP**——服务器连接预设。
- **技能**——能力包位置。
- **基础设施**——沙箱、存储、限额、超时、指标、输出与工具审批策略。
- **服务端**——监听地址、认证、CORS 与限流。

解析优先级依次为：CLI 参数与 SDK 选项、项目级 `.wf/` 文件、全局默认值，最后是处理器内置默认值。

---

## 文档

详细的设计与架构参考位于 `docs/`：

- `docs/architecture/`——Agent 与工作流引擎架构，以及共享基础设施。
- `docs/api/`——门面层与 HTTP 传输层分析。
- `docs/cli/`——CLI 功能集、布局与组件设计。
- `docs/infra/`——LLM 客户端与沙箱架构。
- `docs/foundation/`——类型与插件作者指南。
- `docs/plan/`、`docs/ref/`、`docs/spec/`——规划、参考与规格材料。
