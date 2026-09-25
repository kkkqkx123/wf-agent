# Web App 流式 Markdown 渲染设计方案

> 范围：`apps/web-app`。目标：为 Web 前端引入流式 Markdown 渲染能力，支撑 Agent / Workflow / LLM 生成场景的实时输出展示。
> 后端：`wf-server` 提供 5 个 SSE 端点（见 §2.2）。
> 状态：已落地。传输层、帧分类、流式状态与渲染层均已实现并接入 Agent loop 流；LLM 生成与工作流执行通道已有服务封装、暂未接入页面。本文按当前代码实况撰写。

## 1. 背景与目标

Modular Agent Framework 的核心交互形态是 Agent Loop 与 Workflow 执行，它们的输出以增量文本（Markdown）为主：

- Agent 每轮迭代输出 LLM 回复、工具调用结果、推理过程；
- Workflow 执行输出节点执行日志、LLM delta、错误分析过程；
- 用户直接调用 `/api/v1/llm/generate-stream` 获得流式生成。

Web 前端必须在收到 SSE 帧的同时**增量渲染** Markdown，而非等待完整响应后一次性渲染。这有三个要求：

1. **中间态稳定**：未闭合的代码块、表格、列表不能在流式过程中闪烁或错位；
2. **低抖动**：高频 token delta（每 10–50ms 一帧）不能导致浏览器主线程过载；
3. **功能完整**：支持代码块高亮、Mermaid 图、KaTeX 公式、推理过程折叠展示。

## 2. 技术选型

### 2.1 为什么是 markstream-svelte

项目 `apps/web-app` 使用 **SvelteKit 2 + Svelte 5**（见 [architecture.md](../../apps/web-app/architecture.md)），因此：

| 候选库                  | 框架         | 匹配度 | 说明                                                           |
| ----------------------- | ------------ | ------ | -------------------------------------------------------------- |
| `markstream-vue`        | Vue 3 / Nuxt | ❌     | 同仓库作者的 Vue 包，不兼容 Svelte                             |
| **`markstream-svelte`** | Svelte 5     | ✅     | 同仓库 sibling 包，专为 Svelte 5 设计，API 与 Vue/React 包对齐 |
| `svelte-markdown`       | Svelte       | ⚠️     | 仅支持静态渲染，无流式中间态处理                               |
| `marked` + 手动拼接     | 框架无关     | ⚠️     | 需要自行处理未闭合结构、节流、重渲染成本                       |

**markstream-svelte** 关键点：

- Svelte 5 原生，使用 `$props` 与 runes 语义；
- 流式渲染核心：parser 内部维护 committed / streaming 双缓冲区，未闭合的代码块、表格、引用定义保留在 streaming 区，避免结构抖动；
- 支持 `content` 字符串直接传递（组件内部自行增量解析），`final` prop 收敛未闭合结构；
- 可选 peer dependencies：`stream-diffs`、`mermaid`、`katex`、`@terrastruct/d2`、`@antv/infographic`（**当前均未安装**，库使用内置降级渲染，见 §7）；
- Worker 模式与自定义 HTML 标签注册能力存在，但**实现最终未采用**：推理块改为在状态层与正文分离、由独立组件渲染（见 §5.5），比把 reasoning 文本包进 `<thinking>` 标签再注册组件更直接。

### 2.2 与后端 SSE 协议的关系

后端 5 个 SSE 端点、帧协议与前端接入现状：

| 端点                                                | 帧协议                                                      | 前端封装                                        | 接入状态                   |
| --------------------------------------------------- | ----------------------------------------------------------- | ----------------------------------------------- | -------------------------- |
| `POST /api/v1/agent-loops/{id}/stream`              | `ExecutionStreamEvent`（payload 层 `type` 标签）            | `streamLoopRun`                                 | ✅ 已接入 chat 页          |
| `POST /api/v1/workflows/{id}/execute/stream`        | 首帧协议层 `event: metadata` + `ExecutionStreamEvent`       | `streamWorkflowExecution`                       | 服务封装，未接入页面       |
| `POST /api/v1/llm/generate-stream`                  | `MessageStreamEvent`（payload 层 `event_type` 标签）        | `streamGeneration`                              | 服务封装，未接入页面       |
| `GET /api/v1/executions/{id}/error-analysis/stream` | `ExecutionErrorRecord` 结构化记录（**非** Markdown 文本流） | `streamErrorAnalysis`                           | 服务封装，无 UI 映射       |
| `GET /api/v1/events/stream`                         | `BaseEvent`                                                 | `src/lib/api/sse.ts`（EventSource，live store） | 既有通道，不属于本方案范围 |

**协议约束**（详见 §9）：帧类型标识存在两个来源——执行协议用 serde 内部标签 `type`，LLM 协议用 `event_type`。前端 `frameKind` 同时读取两者做回退；文本增量字段存在别名（`content` / `delta` / `text`），前端按顺序探测。

## 3. 整体架构（实际落地）

```
┌─────────────────────────────────────────────────────────────────┐
│ apps/web-app (SvelteKit)                                        │
│                                                                 │
│  传输层   src/lib/api/stream.ts                                  │
│    openPostStream / openGetStream → onFrame(event, data) 逐帧回调│
│           src/lib/api/sse.ts（既有 GET /events/stream 通道）     │
│                        │                                        │
│  帧分类   src/lib/services/streaming.ts                          │
│    handleStreamFrame：type/event_type → StreamCallbacks 回调面   │
│    streamLoopRun · streamGeneration · streamWorkflowExecution ·  │
│    streamErrorAnalysis                                          │
│                        │                                        │
│  状态层   src/lib/stores/chat-stream.svelte.ts                   │
│    ChatStreamStore：单活动会话；pending 缓冲 + 80ms 定时 flush   │
│    answer / reasoning / tools / subAgents / usage / iteration    │
│                        │ $props 响应式传递                       │
│  渲染层   components/chat/StreamMarkdown.svelte                  │
│            └─ markstream-svelte <MarkdownRender content final    │
│               isDark>                                            │
│          components/chat/ReasoningBlock.svelte（独立折叠块）      │
│          components/domain/ToolCallCard.svelte（工具状态机）      │
│                        │                                        │
│  页面装配 routes/chat/+page.svelte（会话列表 + 时间线 + Composer）│
└─────────────────────────────────────────────────────────────────┘
         │ POST stream · SSE · X-API-Key 头
         ▼
┌─────────────────────────────────────────────────────────────────┐
│ crates/app/wf-server（axum）· 各域 api/.../ 文件内注册路由       │
│ 协议定义：crates/app/wf-api/src/infra/stream.rs                  │
└─────────────────────────────────────────────────────────────────┘
```

## 4. 依赖

`apps/web-app/package.json` 已添加 `markstream-svelte: 2.0.13`（锁定的精确版本），配套样式 `markstream-svelte/index.css` 由 `StreamMarkdown.svelte` 组件内导入。`stream-diffs`、`mermaid`、`katex` 等可选 peer 依赖未安装——缺失时库内部降级（代码块无 diff 增强、mermaid/katex 原样显示源码），不会报错。

## 5. 核心模块实现

### 5.1 传输层（`src/lib/api/stream.ts`）

职责：提供可取消的 POST / GET SSE 读取器，向调用方逐帧回调 `(event, data)`。

- **POST SSE 走 fetch + ReadableStream**：浏览器原生 `EventSource` 不支持 POST，回路 / 生成 / 工作流执行三个通道均需 `response.body.getReader()` 手动泵读；`AbortController.signal` 贯穿始终，取消即断开（服务端 drop 流时中止驱动任务）。
- **帧解析**：按 `\n\n` 切帧，帧内逐行识别 `event:`（协议层事件名）与 `data:`（JSON 解析，失败则原样传字符串；`[DONE]` 哨兵跳过）。
- **鉴权**：POST 注入 `x-api-key` 请求头；GET 因 EventSource 头限制走 `api_key` query 参数。密钥统一由 `$lib/api/client.ts` 的 `resolveApiKey()` 解析（env 回退 settings store）。
- **错误映射**：非 2xx 响应读取 envelope 的 `error.message`，回退 `Stream failed (HTTP <status>)`；通过 `onError` 回调上报。
- **重连**：**未实现**自动指数退避。失败即终止并进入可重试错误态，与 live store 的手动重试策略一致（列入 §8 遗留项）。

### 5.2 帧分类（`src/lib/services/streaming.ts`）

职责：把传输层帧路由到 `StreamCallbacks` 回调面（`onDelta` / `onReasoning` / `onIterationStart` / `onIterationEnd` / `onToolStart` / `onToolEnd` / `onUsage` / `onSubAgent` / `onCompleted` / `onFailed` / `onInterrupted` / `onError`）。

- `frameKind`：优先 payload `type`（执行协议），回退 `event_type`（LLM 协议）；
- `frameText`：按 `content` → `delta` → `text` 顺序取增量文本；
- **未知帧忽略**、`engine` 帧跳过（生命周期事件由 events 通道覆盖）、协议层 `event === 'metadata'` 首帧跳过——保证服务端新增事件类型不破坏前端时间线；
- 四个通道函数：`streamLoopRun`（POST body 携带 `model` / `message` / `conversation` 历史与 `tool_call_protocol: {format:'json'}`）、`streamGeneration`、`streamWorkflowExecution`、`streamErrorAnalysis`（GET）。

### 5.3 状态层（`src/lib/stores/chat-stream.svelte.ts`）

职责：维护**单个活动会话**的流式状态并节流渲染。`class ChatStreamStore`，Svelte 5 runes（`$state` 字段 + 顶层单例导出）。

- 字段：`sessionKey`、`active`、`answer`（正文 Markdown 累计）、`reasoning`、`tools: LiveToolCall[]`（`pending → running → completed / failed` 状态机）、`subAgents`、`usage`、`iteration`、`done`、`error`；
- 缓冲：`pending` / `reasoningPending` 私有字符串，帧到达只追加（零解析开销）；`setInterval(80ms)` 定时 `flush()` 合并进 `answer` / `reasoning`，触发 `MarkdownRender` 增量重解析；常量 `STREAM_FLUSH_MS = 80` 导出可调；
- 生命周期：`start(sessionKey)` 复位状态、起计时器、返回 `AbortSignal`；`complete()` / `fail(msg)` 先 flush 再收敛终态；`stop()` 中止 fetch 并清理计时器。

**与早期设计的差异**：原 §5.2 设想"多会话 `Record<id, StreamSession>` + 每消息 `pendingDelta` + 50ms rAF flush"。实际按产品形态收敛为**单活动会话、双流缓冲、80ms 定时器**——页面同一时刻只允许一条流；多会话并发留待真实需求出现时再做。

### 5.4 渲染层（`src/lib/components/chat/StreamMarkdown.svelte`）

职责：封装 markstream-svelte，对外只暴露三项：`content`（完整 Markdown 文本）、`done`（流结束信号 → 组件内映射为库的 `final` prop）、以及**内部自取的** `isDark`（由 `$lib/stores/theme.svelte` 的 `resolvedTheme` 派生，不由外部传入）。组件内导入 `markstream-svelte/index.css`。

`final=true` 时库把所有 streaming 区内容强制收敛为 committed，正好对应后端 `completed` / `failed` 终态。

### 5.5 推理块与工具卡

- **ReasoningBlock**（`components/chat/ReasoningBlock.svelte`）：`details` 式可折叠面板，props 为 `content` 与 `streaming`；`reasoning_delta` 帧的文本在状态层与正文分离，不进 Markdown 主流。内部用 `StreamMarkdown` 渲染 reasoning 自身的 Markdown。
- **ToolCallCard**（`components/domain/ToolCallCard.svelte`）：`tool_start` / `tool_end` 帧渲染为带状态指示的卡片。工具调用**不经过** Markdown 渲染器（非 Markdown 语法元素）。注意执行协议的 `tool_start` 帧只携带 `tool_call_id` / `tool_name`（**无参数**），完整 `result` 与 `error` 在 `tool_end` 帧到达。

## 6. SSE 帧 → UI 渲染映射

### 6.1 执行协议 `ExecutionStreamEvent`（agent loop / workflow 共用）

协议定义：`crates/app/wf-api/src/infra/stream.rs`，serde `tag = "type"`、snake_case。

| type                                    | payload 字段                                           | UI 动作                            |
| --------------------------------------- | ------------------------------------------------------ | ---------------------------------- |
| `llm_delta`                             | `{ content }`                                          | `appendDelta` → 缓冲 → `answer`    |
| `reasoning_delta`                       | `{ content }`                                          | `appendReasoning` → ReasoningBlock |
| `iteration_start` / `iteration_end`     | `{ iteration, message_count, array_version }`          | 更新迭代计数（折叠容器未做）       |
| `tool_start`                            | `{ tool_call_id, tool_name }`                          | 新增 ToolCallCard（running）       |
| `tool_end`                              | `{ tool_call_id, tool_name, success, result, error? }` | 卡片终态 + 完整结果                |
| `usage`                                 | `{ prompt_tokens, completion_tokens, cost? }`          | 流式区 token/费用计数              |
| `sub_agent_started` / `sub_agent_ended` | `{ id, name, success? }`                               | 子代理标注                         |
| `completed`                             | `{ result, iterations }`                               | `complete()`：收敛 + 写历史        |
| `failed` / `interrupted`                | `{ error }` / `{ reason }`                             | `fail()`：错误态                   |
| `engine`                                | `BaseEvent`                                            | 忽略（events 通道覆盖）            |

**背压语义**：服务端对非终态帧 `try_send`，通道满时丢弃（不阻塞引擎）；终态帧必达。前端因此不能假设帧序列无损，终态以 `completed` / `failed` 为准。

### 6.2 Workflow execute stream 差异

首帧为协议层 `event: metadata` + `data: { "execution_id" }`（用于把执行 id 告知客户端），其余帧与 6.1 词表相同。前端在帧分类层统一跳过 `metadata`。

### 6.3 LLM generate stream（`MessageStreamEvent`）

协议定义：`crates/foundation/wf-types/src/llm/message_stream_events.rs`，serde `tag = "event_type"`。

| event_type                | payload                   | 当前前端行为                                                  |
| ------------------------- | ------------------------- | ------------------------------------------------------------- |
| `text`                    | `{ text, snapshot }`      | 命中默认分支 → `onDelta`（取 `text` 增量；`snapshot` 未使用） |
| `error`                   | `{ error }`               | `onError`                                                     |
| `end`                     | `{}`                      | 忽略（传输层 EOF 收敛终态）                                   |
| `reasoning_text`          | `{ reasoning, snapshot }` | **未区分**——落入默认分支后无别名文本被忽略                    |
| `usage` / `final_message` | 嵌套结构                  | **未映射**（字段形状与执行协议 `usage` 不同）                 |

即 `streamGeneration` 目前只能正确渲染正文增量；接入页面前需为 `reasoning_text` / `usage` / `final_message` 补分支（列入 §8 遗留项）。

### 6.4 限速策略

```
后端 SSE 帧到达（10–50ms/帧）
    ▼  零解析开销：仅追加 pending / reasoningPending 缓冲
ChatStreamStore 内部字段
    ▼  setInterval 每 80ms 一次 flush()
answer / reasoning 更新 → MarkdownRender 增量重解析（committed/streaming 双缓冲）
```

80ms 阈值兼顾视觉流畅与主线程负载（约每 80ms 一次重解析，而非每帧一次）；导出常量 `STREAM_FLUSH_MS` 供调整，未做环境变量注入。

## 7. 可选增强能力（均未引入）

| 能力                | 依赖                                   | 现状                                     |
| ------------------- | -------------------------------------- | ---------------------------------------- |
| 代码块 diff 增强    | `stream-diffs`                         | 未安装，库内置代码块渲染                 |
| Mermaid 图          | `mermaid`                              | 未安装，代码块原样显示；可配 Worker 模式 |
| KaTeX 公式          | `katex`                                | 未安装，同上                             |
| D2 / Infographic    | `@terrastruct/d2`、`@antv/infographic` | 未安装                                   |
| 断点续拉 / 自动重连 | —                                      | 未实现，失败为手动重试                   |
| WebSocket 并行订阅  | 原生 WS                                | 未接入                                   |

## 8. 实施状态

### 已完成（对应早期阶段 1 / 部分阶段 2）

| 事项                                               | 落地文件                                        |
| -------------------------------------------------- | ----------------------------------------------- |
| `markstream-svelte` 依赖引入（2.0.13）             | `apps/web-app/package.json`                     |
| POST/GET 可取消 SSE 传输层                         | `src/lib/api/stream.ts`                         |
| 帧分类 + 四通道封装                                | `src/lib/services/streaming.ts`                 |
| 流式状态 + 80ms flush                              | `src/lib/stores/chat-stream.svelte.ts`          |
| Markdown 封装（content/done + 主题内取）           | `src/lib/components/chat/StreamMarkdown.svelte` |
| 推理折叠块（独立组件，替代 `<thinking>` 标签方案） | `src/lib/components/chat/ReasoningBlock.svelte` |
| 工具卡状态机 + 完整结果                            | `src/lib/components/domain/ToolCallCard.svelte` |
| Agent loop 流接入                                  | `src/routes/chat/+page.svelte`                  |
| 错误帧统一错误态                                   | `src/lib/components/ui/ErrorState.svelte`       |

### 遗留项

1. `streamGeneration` 的 `reasoning_text` / `usage` / `final_message` 帧映射 + 页面接入；
2. `streamWorkflowExecution` 接入 workflow 执行页（节点级 UI）；
3. `streamErrorAnalysis` 的结构化记录展示（需与 `ExecutionErrorRecord` 契约对齐）；
4. 迭代折叠容器（`iteration_*` 帧已入状态、未成 UI）；
5. 传输层自动重连 / 断点续拉；
6. 多会话并发流（当前单活动会话）。

## 9. 后端协议注意事项

当前后端 SSE 帧均**不带** `event:` 前缀（唯一例外：workflow execute stream 首帧 `event: metadata`），业务负载全部放在 `data:` 行的 JSON 里。路由与序列化位置：

- `/agent-loops/{id}/stream`：`crates/app/wf-server/src/api/agent/loops.rs`（`handle_stream_loop`，序列化 `ExecutionStreamEvent`）；
- `/llm/generate-stream`：`crates/app/wf-server/src/api/llm.rs`（`handle_generate_stream`，序列化 `MessageStreamEvent`）；
- `/workflows/{id}/execute/stream`：`crates/app/wf-server/src/api/workflow/executions.rs`（`handle_execute_stream`）；
- `/executions/{id}/error-analysis/stream`：`crates/app/wf-server/src/api/observation/analysis.rs`（`handle_error_chain_stream`）。

帧类型识别需要同时兼容 payload 层 `type`（执行协议）与 `event_type`（LLM 协议）两个来源——前端 `frameKind` 已按此实现，不阻塞后端演进。

## 10. 与 TUI 端 MarkdownStream 的关系

项目 TUI 端（`crates/app/tui/tui-markdown/src/markdown/stream.rs`）已有成熟的流式 Markdown 解析器 `MarkdownStream`，采用 committed / streaming 双缓冲区 + 增量边界检测（避免重解析已提交内容）。

Web 端引入 markstream-svelte 后两端的关系：

| 维度     | TUI MarkdownStream                   | Web markstream-svelte                            |
| -------- | ------------------------------------ | ------------------------------------------------ |
| 解析器   | pulldown-cmark（Rust）               | stream-markdown-parser（JS/TS）                  |
| 运行位置 | 进程内，零网络开销                   | 浏览器 JS runtime                                |
| 增量策略 | 自行实现 committed / streaming split | 内部同构实现                                     |
| 能力     | 纯文本渲染，代码块语言识别           | HTML 渲染，Mermaid/KaTeX/D2/stream-diffs（可选） |
| 代码共享 | —                                    | 不共享，两端独立                                 |

结论：两端**不共享代码**，但可参考 TUI 端的 holdback 规则（引用定义后置、表格不闭合、代码块 fence 检测）来验证前端行为的正确性。markstream-svelte 的内部实现已涵盖这些规则，前端只需正确传递 `content` 与 `final` props。

## 11. 关联文档

| 文档                                                                     | 职责                             |
| ------------------------------------------------------------------------ | -------------------------------- |
| `docs/apps/web-app/architecture.md`                                      | Web 前端整体架构（必读）         |
| `docs/plan/web/chat-ide-primary-design.md`                               | 对话 IDE 产品形态                |
| `docs/plan/web-frontend-renovation-phases.md`                            | 分阶段实施计划（本方案属阶段 2） |
| `docs/api/08-wf-server-HTTP层.md`                                        | HTTP/SSE 协议细节                |
| `docs/apps/web-app/svelte-best-practice.md`                              | Svelte 5 开发约定                |
| `crates/app/wf-api/src/infra/stream.rs`                                  | 执行流协议定义                   |
| `crates/foundation/wf-types/src/llm/message_stream_events.rs`            | LLM 流协议定义                   |
| `crates/app/tui/tui-markdown/src/markdown/stream.rs`                     | TUI 端流式解析器参考             |
| [markstream-svelte npm](https://www.npmjs.com/package/markstream-svelte) | 官方包文档                       |
