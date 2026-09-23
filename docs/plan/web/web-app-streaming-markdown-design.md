# Web App 流式 Markdown 渲染设计方案

> 范围：`apps/web-app`。目标：为 Web 前端引入流式 Markdown 渲染能力，支撑 Agent / Workflow / LLM 生成场景的实时输出展示。
> 后端：`wf-server` 已提供 5 个 SSE 端点（见架构文档第 6 节）。
> 状态：设计中。

## 1. 背景与目标

Modular Agent Framework 的核心交互形态是 Agent Loop 与 Workflow 执行，它们的输出以增量文本（Markdown）为主：

- Agent 每轮迭代输出 LLM 回复、工具调用结果、推理过程；
- Workflow 执行输出节点执行日志、LLM delta、错误分析过程；
- 用户直接调用 `/api/v1/llm/generate-stream` 获得流式生成。

Web 前端必须在收到 SSE 帧的同时**增量渲染** Markdown，而非等待完整响应后一次性渲染。这有三个要求：

1. **中间态稳定**：未闭合的代码块、表格、列表不能在流式过程中闪烁或错位；
2. **低抖动**：高频 token delta（每 10–50ms 一帧）不能导致浏览器主线程过载；
3. **功能完整**：支持代码块高亮、Mermaid 图、KaTeX 公式、自定义组件（如 `<thinking>` 折叠块）。

## 2. 技术选型

### 2.1 为什么是 markstream-svelte

项目 `apps/web-app` 使用 **SvelteKit 2 + Svelte 5**（见 [architecture.md](../apps/web-app/architecture.md)），因此：

| 候选库 | 框架 | 匹配度 | 说明 |
|---|---|---|---|
| `markstream-vue` | Vue 3 / Nuxt | ❌ | 同仓库作者的 Vue 包，不兼容 Svelte |
| **`markstream-svelte`** | Svelte 5 | ✅ | 同仓库 sibling 包，专为 Svelte 5 设计，API 与 Vue/React 包对齐 |
| `svelte-markdown` | Svelte | ⚠️ | 仅支持静态渲染，无流式中间态处理 |
| `marked` + 手动拼接 | 框架无关 | ⚠️ | 需要自行处理未闭合结构、节流、重渲染成本 |

**markstream-svelte** 关键点：

- Svelte 5 原生，使用 `$props` 与 runes 语义；
- 流式渲染核心：parser 内部维护 committed / streaming 双缓冲区，未闭合的代码块、表格、引用定义保留在 streaming 区，避免结构抖动；
- 支持 `content` 字符串直接传递（组件内部自行增量解析），也支持预解析的 `nodes` AST；
- 可选 peer dependencies：`stream-diffs`（增强代码块）、`mermaid`、`katex`、`@terrastruct/d2`、`@antv/infographic`；
- Worker 模式：KaTeX / Mermaid 可放入 Web Worker，避免阻塞主线程；
- 自定义 HTML 标签注册：如 `<thinking>` → 自定义折叠组件。

### 2.2 与后端 SSE 协议的关系

后端已实现的 5 个 SSE 端点（见架构文档 §6.1）：

| 端点 | 内容类型 | Markstream 接入方式 |
|---|---|---|
| `POST /api/v1/llm/generate-stream` | 纯 LLM token delta | delta → content 字符串累加 → `<MarkdownRender>` |
| `POST /api/v1/agent-loops/{id}/stream` | ExecutionStreamEvent | `llm_delta` / `reasoning_delta` → 分别渲染 assistant 回复与推理块 |
| `POST /api/v1/workflows/{id}/execute/stream` | ExecutionStreamEvent | 同 agent loop，按 `type` 路由到不同 UI 区域 |
| `GET /api/v1/events/stream` | BaseEvent | 事件时间线，仅展示元数据，非 Markdown 主体 |
| `GET /api/v1/executions/{id}/error-analysis/stream` | 分析文本流 | 类似 LLM generate-stream，单段 Markdown |

**协议约束**：当前后端 SSE 帧结构为 `{ "event_type": "...", ...payload... }`（见 [llm.rs](../../crates/app/wf-server/src/api/llm/llm.rs#L132-L149)），其中 `llm_delta` 帧的 payload 包含增量文本片段。前端 SSE 客户端需解析帧并将文本累加到对应消息的 content 缓冲区。

## 3. 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│ apps/web-app (SvelteKit)                                       │
│                                                                 │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │  src/lib/api/sse.ts                                        │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐ │ │
│  │  │ SSEClient    │  │ StreamParser │  │ FrameAccumulator │ │ │
│  │  │ (连接管理)    │  │ (帧解析/类型) │  │ (文本拼接/限速)  │ │ │
│  │  └──────────────┘  └──────────────┘  └──────────────────┘ │ │
│  └─────────────────────────┬──────────────────────────────────┘ │
│                            │ Svelte store 写入                   │
│  ┌─────────────────────────▼──────────────────────────────────┐ │
│  │  src/lib/stores/stream.ts                                  │ │
│  │  { messages: Map<id, { content, isDone, reasoning, ... }> }│ │
│  └─────────────────────────┬──────────────────────────────────┘ │
│                            │ $props 响应式传递                    │
│  ┌─────────────────────────▼──────────────────────────────────┐ │
│  │  src/lib/components/markdown/                              │ │
│  │  ┌────────────────────────────────────────────────────────┐ │ │
│  │  │ StreamingMarkdown.svelte                               │ │ │
│  │  │  └─ markstream-svelte <MarkdownRender>                 │ │ │
│  │  │     props: { content, isDone, codeTheme, customIds }  │ │ │
│  │  └────────────────────────────────────────────────────────┘ │ │
│  │  ┌───────────────┐  ┌──────────────┐  ┌────────────────┐  │ │
│  │  │ ThinkingNode  │  │ ToolCallNode │  │ CodeBlockNode  │  │ │
│  │  │ (自定义组件)   │  │ (工具调用展示) │  │ (stream-diffs) │  │ │
│  │  └───────────────┘  └──────────────┘  └────────────────┘  │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                 │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │  页面装配层 (routes/...)                                   │ │
│  │  chat/+page.svelte · executions/[id]/+page.svelte · ...    │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
         │ POST /api/v1/llm/generate-stream · SSE
         │ POST /api/v1/agent-loops/{id}/stream · SSE
         ▼
┌─────────────────────────────────────────────────────────────────┐
│ crates/app/wf-server (axum)                                     │
│  sse_response() · llm::handle_generate_stream · workflow::...  │
└─────────────────────────────────────────────────────────────────┘
```

## 4. 依赖引入

在 `apps/web-app/package.json` 中添加：

```json
{
  "dependencies": {
    "markstream-svelte": "^2.0.0",
    "stream-diffs": "^0.2.0"
  },
  "optionalDependencies": {
    "mermaid": "^11.0.0",
    "katex": "^0.16.0"
  }
}
```

版本说明：`markstream-svelte` 当前稳定版本为 2.0.x（2026-09 同步发布），需 Svelte 5（项目已使用 5.56.4）。

## 5. 核心模块设计

### 5.1 SSE 客户端 (`src/lib/api/sse.ts`)

职责：封装浏览器 SSE 连接，处理 POST SSE（非原生 EventSource）、鉴权头、重连、帧解析、限速推送。

```typescript
// src/lib/api/sse.ts 接口设计

// SSE 帧原始结构
interface SseFrame {
  event?: string;
  data: string;
}

// 解析后的业务事件（按后端 ExecutionStreamEvent 约定）
type StreamEventType =
  | 'metadata'        // workflow 执行流首帧
  | 'llm_delta'       // LLM token 增量
  | 'reasoning_delta' // 推理 token 增量
  | 'tool_start'
  | 'tool_end'
  | 'iteration_start'
  | 'iteration_end'
  | 'completed'
  | 'failed'
  | 'interrupted'
  | 'usage'
  | 'engine'
  | 'error'
  | string;

interface StreamEvent {
  type: StreamEventType;
  payload: Record<string, unknown>;
}

// SSE 客户端
interface SseClient {
  // POST SSE（需要 fetch + ReadableStream，不能用原生 EventSource）
  postStream(
    url: string,
    body: unknown,
    options?: { signal?: AbortSignal; headers?: Record<string, string> }
  ): AsyncIterable<StreamEvent>;

  // GET SSE（原生 EventSource）
  getStream(
    url: string,
    options?: { signal?: AbortSignal; query?: Record<string, string> }
  ): EventSource;
}
```

**关键实现点**：

1. **POST SSE 走 fetch + ReadableStream**：浏览器原生 `EventSource` 只支持 GET。`POST /api/v1/llm/generate-stream` 和 `POST /api/v1/workflows/{id}/execute/stream` 需用 `fetch` 获取 `response.body.getReader()`，按 `\n\n` 分割帧。
2. **帧解析**：标准 SSE 格式为 `event: xxx\ndata: {...}\n\n`。`data` 行拼合后 JSON parse。后端当前发送的帧不带 `event:` 前缀（除 workflow 首帧 `event: metadata`），全部放在 `data:` 里。
3. **鉴权头**：同源部署时 `x-api-key` 头由 Vite 代理或直接 fetch 注入；独立部署时读取 `VITE_API_KEY`。
4. **重连策略**：指数退避（1s → 2s → 4s，上限 30s），携带 `since` 游标（如果后端支持断点续拉）。
5. **限速推送**：帧到达时仅写入 store 的缓冲区；每 50ms（可配置）用 `requestAnimationFrame` 或 `setTimeout` 批量 flush 一次，避免每帧都触发 markstream-svelte 的重新解析。

### 5.2 Stream Store (`src/lib/stores/stream.ts`)

职责：维护多条流式会话的状态，为 UI 组件提供响应式订阅。

```typescript
// 单条流式消息
interface StreamingMessage {
  id: string;
  role: 'assistant' | 'user' | 'system' | 'tool';
  content: string;        // 累加到目前的完整 Markdown 文本
  reasoning?: string;     // 推理块文本（独立渲染区域）
  isDone: boolean;        // 是否已结束（对应后端 completed / failed）
  error?: string;
  toolCalls?: ToolCallState[];
  // 内部：自上次 flush 后新增的 delta
  pendingDelta: string;
}

// 会话级状态
interface StreamSession {
  id: string;             // execution_id 或 agent_loop_id
  messages: StreamingMessage[];
  status: 'connecting' | 'streaming' | 'completed' | 'failed' | 'interrupted';
  startedAt: number;
}

// Store 接口
interface StreamStore {
  sessions: Record<string, StreamSession>;

  startSession(params: {
    sessionId: string;
    endpoint: string;
    method: 'GET' | 'POST';
    body?: unknown;
    initialMessage?: Partial<StreamingMessage>;
  }): Promise<void>;

  cancelSession(sessionId: string): void;
}
```

**设计要点**：

- **Svelte 5 store 模式**：使用 `$state()` 和 `$derived()`，而非传统的 writable store。Svelte 5 runes 在组件层级外也可使用（需要 `.svelte.ts` 后缀或 `$state.frozen`）。
- **每条消息独立 content 缓冲区**：`llm_delta` 帧按 `message_id` 或 `role` 路由到对应消息。assistant 回复和 reasoning 块**分开维护**，因为它们渲染在不同的 UI 区域。
- **flush 节流**：每 50ms 调用一次 `flushPendingDelta(sessionId)`，将 `pendingDelta` 追加到 `content` 并清空，触发 markstream-svelte 重新解析。

### 5.3 StreamingMarkdown 组件 (`src/lib/components/markdown/StreamingMarkdown.svelte`)

职责：封装 markstream-svelte，对外暴露稳定 props，处理主题、代码块、自定义组件注册。

```svelte
<!-- StreamingMarkdown.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import MarkdownRender, { setCustomComponents } from 'markstream-svelte';
  import 'markstream-svelte/index.css';
  // 可选增强
  import 'katex/dist/katex.min.css';

  import ThinkingNode from './nodes/ThinkingNode.svelte';

  let {
    content = '',        // 当前完整 Markdown 文本
    isDone = false,      // 流是否结束
    isDark = false,      // 暗色主题
    showCodeHeader = true,
    customComponents = {}, // 用户自定义节点
  }: {
    content?: string;
    isDone?: boolean;
    isDark?: boolean;
    showCodeHeader?: boolean;
    customComponents?: Record<string, any>;
  } = $props();

  // 注册自定义组件
  const customId = $derived.by(() => {
    const id = `session-${Math.random().toString(36).slice(2)}`;
    setCustomComponents(id, { thinking: ThinkingNode, ...customComponents });
    return id;
  });
</script>

<MarkdownRender
  content={content}
  final={isDone}
  isDark={isDark}
  {customId}
  customHtmlTags={['thinking', ...Object.keys(customComponents)]}
/>
```

**关键 props 映射**：

| StreamingMarkdown prop | markstream-svelte prop | 说明 |
|---|---|---|
| `content` | `content` | 完整 Markdown 字符串，组件内部自行增量解析 |
| `isDone` | `final` | 流结束信号，`final=true` 时组件收敛所有未闭合结构 |
| `isDark` | `isDark` | 暗色主题 |
| `customComponents` | 通过 `setCustomComponents` + `customId` | 自定义 HTML 标签 → 组件映射 |

**`final` prop 的作用**：markstream-svelte 接收 `final=true` 时，会将所有 streaming 区的内容强制收敛为 committed，渲染最终确定态。这正好对应后端 `completed` / `failed` 事件。

### 5.4 节点组件 (`src/lib/components/markdown/nodes/`)

#### ThinkingNode — 推理块折叠

后端 `reasoning_delta` 帧携带的文本应包裹在 `<thinking>` 自定义标签内传递给 markstream-svelte，渲染为可折叠面板：

```svelte
<!-- ThinkingNode.svelte -->
<script lang="ts">
  import MarkdownRender from 'markstream-svelte';

  let { node, customId }: { node: any; customId?: string } = $props();
  let open = $state(false);
  let content = $derived(String(node?.content ?? ''));
</script>

<details class="thinking-node" bind:open>
  <summary class="thinking-summary">
    <span>🧠 推理过程</span>
    {#if content && !content.trim().endsWith('```') && !open}
      <span class="thinking-indicator">生成中…</span>
    {/if}
  </summary>
  <div class="thinking-content">
    <MarkdownRender {content} {customId} final={false} />
  </div>
</details>
```

#### ToolCallNode — 工具调用展示

`tool_start` / `tool_end` 帧渲染为带状态指示的卡片（pending → running → done/failed）。这部分**不经过** markstream-svelte，而是独立的业务组件，因为工具调用不是 Markdown 语法元素。

### 5.5 页面装配示例

#### Chat 页面 (`routes/chat/+page.svelte`)

```svelte
<script lang="ts">
  import { streamStore } from '$lib/stores/stream';
  import StreamingMarkdown from '$lib/components/markdown/StreamingMarkdown.svelte';

  let input = $state('');
  let isDark = $state(false);

  async function sendMessage() {
    const sessionId = crypto.randomUUID();
    streamStore.startSession({
      sessionId,
      endpoint: '/api/v1/llm/generate-stream',
      method: 'POST',
      body: {
        profile_id: 'default',
        messages: [/* ... */],
      },
    });
  }
</script>

<main>
  {#each Object.values(streamStore.sessions) as session}
    {#each session.messages as msg}
      <div class="message {msg.role}">
        {#if msg.role === 'assistant' || msg.role === 'tool'}
          <StreamingMarkdown
            content={msg.content}
            isDone={msg.isDone}
            isDark={isDark}
          />
        {:else}
          <p>{msg.content}</p>
        {/if}
      </div>
    {/each}
  {/each}

  <textarea bind:value={input} />
  <button onclick={sendMessage}>Send</button>
</main>
```

## 6. SSE 帧 → UI 渲染映射

### 6.1 LLM Generate Stream

后端帧 payload（根据 `wf-api::llm` 实现）：

| 帧类型 | payload 字段 | UI 动作 |
|---|---|---|
| token_delta | `{ content: "hello" }` | 追加到当前 assistant 消息的 `content` 缓冲区 |
| reasoning_delta | `{ content: "let me think..." }` | 追加到 `<thinking>` 块的 `content`，渲染为 ThinkingNode |
| usage | `{ prompt_tokens, completion_tokens }` | 更新顶部 token 计数 |
| completed | `{ final_content: "完整文本" }` | 可选：用服务端最终文本覆盖本地缓冲区做确定性收敛；设置 `isDone=true` |
| error | `{ message: "..." }` | 设置 `isDone=true`，`error` 字段写入，展示错误态 |

### 6.2 Agent Loop Stream

多了工具调用相关帧：

| 帧类型 | payload | UI 动作 |
|---|---|---|
| tool_start | `{ tool_name, args, call_id }` | 新增 ToolCallNode（status=running） |
| tool_end | `{ call_id, result, error? }` | 更新 ToolCallNode，追加 tool 消息 |
| iteration_start | `{ iteration, max_iterations }` | 更新迭代计数 |
| iteration_end | — | 可折叠本轮内容 |
| sub_agent_started / sub_agent_ended | `{ sub_agent_id }` | 嵌套会话展示 |

### 6.3 限速策略

高频 token delta（LLM 可达 50–200ms/帧）直接写 store 会导致 markstream-svelte 内部 parser 每帧全量重跑。两级限速：

```
后端 SSE 帧到达
    │ 10–50ms
    ▼
┌──────────────────────────┐
│ SSEClient.onFrame()      │
│  仅写入 session.messages │
│  [i].pendingDelta += txt │  ← 零解析开销
└────────────┬─────────────┘
             │
             │ 每 50ms rAF 触发
             ▼
┌──────────────────────────┐
│ StreamStore.flushPending │
│  content += pendingDelta │
│  pendingDelta = ""       │  ← 触发 markstream-svelte 重新解析
└──────────────────────────┘
             │
             ▼
┌──────────────────────────┐
│ markstream-svelte        │
│  内部 committed /        │
│  streaming 增量解析       │
└──────────────────────────┘
```

**50ms 阈值依据**：60fps 屏幕约 16.7ms/帧，50ms ≈ 3 帧间隔，兼顾视觉流畅与主线程负载。可通过 `STREAM_FLUSH_INTERVAL_MS` 环境变量调优。

## 7. 可选增强能力

以下能力按需引入，不在第一阶段强制：

| 能力 | 依赖 | 价值 | 接入点 |
|---|---|---|---|
| 代码块增强（diff / 选择） | `stream-diffs` | Agent 生成代码时的变更 diff 可视化 | StreamingMarkdown `codeBlockOptions` prop |
| Mermaid 图渲染 | `mermaid` | 架构图、流程图 | 可选 worker 模式 `setMermaidWorker` |
| KaTeX 公式 | `katex` | 数学公式 | 可选 worker 模式 `setKaTeXWorker` |
| D2 图 | `@terrastruct/d2` | 云架构图 | markstream-svelte 自动检测 peer dep |
| Infographic | `@antv/infographic` | 信息图 | 同上 |
| 断点续拉 | — | SSE 断线恢复 | SSEClient 保存 `lastEventId`，重连时传 `?since=xxx` |
| WebSocket 并行订阅 | 原生 WS | 多执行同时观察 | 复用架构文档 §6.2 WS 端点 |

## 8. 实施计划

### 阶段 1：基础流式渲染（核心路径）

| 序号 | 任务 | 产出文件 |
|---|---|---|
| 1.1 | 安装 `markstream-svelte` + `stream-diffs` 依赖 | `apps/web-app/package.json` |
| 1.2 | 创建 SSE 客户端（fetch + ReadableStream POST / EventSource GET） | `src/lib/api/sse.ts` |
| 1.3 | 创建 Stream Store（Svelte 5 runes，会话 + 消息状态管理） | `src/lib/stores/stream.svelte.ts` |
| 1.4 | 创建 StreamingMarkdown 组件（封装 markstream-svelte + CSS） | `src/lib/components/markdown/StreamingMarkdown.svelte` |
| 1.5 | 创建 ThinkingNode 自定义组件 | `src/lib/components/markdown/nodes/ThinkingNode.svelte` |
| 1.6 | 实现 flush 节流（50ms rAF 批处理） | `stream.svelte.ts` 内 flush 逻辑 |
| 1.7 | 对接 `POST /api/v1/llm/generate-stream` 做联调 | 临时调试页面 |

### 阶段 2：Agent Loop / Workflow 执行流

| 序号 | 任务 | 产出 |
|---|---|---|
| 2.1 | 实现 ToolCallNode 组件（运行状态 + 结果展示） | `nodes/ToolCallNode.svelte` |
| 2.2 | 实现迭代循环折叠容器 | `nodes/IterationNode.svelte` |
| 2.3 | 对接 `POST /api/v1/agent-loops/{id}/stream` | agent 执行详情页 |
| 2.4 | 对接 `POST /api/v1/workflows/{id}/execute/stream` | workflow 执行详情页 |
| 2.5 | 帧类型错误处理（error 帧 → 红色告警态） | 统一错误 UI |

### 阶段 3：增强能力

| 序号 | 任务 | 说明 |
|---|---|---|
| 3.1 | 引入 Mermaid + KaTeX worker | 可选 peer deps + Vite worker 导入 |
| 3.2 | 代码块 stream-diffs 增强 | 大模型生成 patch 时显示 diff |
| 3.3 | 断点续拉 + 重连策略 | SSEClient 增强 |
| 3.4 | WebSocket 多订阅 | 架构文档 §6.2 端点 |

## 9. 后端协议注意事项

当前后端 SSE 帧在 `handle_generate_stream` 中**没有**发送 `event:` 字段（见 [llm.rs](../../crates/app/wf-server/src/api/llm/llm.rs#L132-L149)），所有事件都在 `data:` 行里以 `{"event_type": "...", ...}` JSON 负载承载。前端需兼容此约定：

1. **帧类型识别优先从 payload 的 `event_type` 字段读取**，而非 SSE 协议层的 `event:` 前缀；
2. 仅 `workflow/executions` 流首帧显式使用 `event: metadata`（见 [executions.rs](../../crates/app/wf-server/src/api/workflow/executions.rs#L124-L126)），前端需同时处理协议层 `event:` 和 payload 层 `event_type` 两个来源；
3. **建议后端后续统一**：要么全部使用 SSE `event:` 前缀（前端解析更简洁），要么在 payload 中保留 `event_type`。本设计按**兼容两种**处理，不阻塞后端演进。

## 10. 与 TUI 端 MarkdownStream 的关系

项目 TUI 端（`crates/app/tui/tui-markdown/src/markdown/stream.rs`）已有成熟的流式 Markdown 解析器 `MarkdownStream`，采用 committed / streaming 双缓冲区 + 增量边界检测（避免重解析已提交内容）。

Web 端引入 markstream-svelte 后两端的关系：

| 维度 | TUI MarkdownStream | Web markstream-svelte |
|---|---|---|
| 解析器 | pulldown-cmark（Rust） | stream-markdown-parser（JS/TS） |
| 运行位置 | 进程内，零网络开销 | 浏览器 JS runtime |
| 增量策略 | 自行实现 committed / streaming split | 内部同构实现 |
| 能力 | 纯文本渲染，代码块语言识别 | HTML 渲染，Mermaid/KaTeX/D2/stream-diffs |
| 代码共享 | — | 不共享，两端独立 |

结论：两端**不共享代码**，但可参考 TUI 端的 holdback 规则（引用定义后置、表格不闭合、代码块 fence 检测）来验证前端行为的正确性。markstream-svelte 的内部实现已涵盖这些规则，前端只需正确传递 `content` 与 `final` props。

## 11. 关联文档

| 文档 | 职责 |
|---|---|
| `docs/apps/web-app/architecture.md` | Web 前端整体架构（必读） |
| `docs/plan/web/frontend-feature-list.md` | 功能清单 |
| `docs/api/08-wf-server-HTTP层.md` | HTTP/SSE 协议细节 |
| `docs/apps/web-app/svelte-best-practice.md` | Svelte 5 开发约定 |
| `crates/app/tui/tui-markdown/src/markdown/stream.rs` | TUI 端流式解析器参考 |
| [markstream-svelte npm](https://www.npmjs.com/package/markstream-svelte) | 官方包文档 |
| [markstream 文档](https://markstream.simonhe.me/guide/svelte) | Svelte 包官方指南 |
