# Web App 前端 API 正式接线设计方案

> 配套：`docs/plan/web-app-integration.md`（整体集成与 codegen 管线）、`docs/plan/openapi-schema-typing.md`（后端响应体类型化分阶段）、`docs/plan/web/web-app-ui-implementation-design.md`（已落地的样式/组件）。
> 目标：将 `apps/web-app` 从"纯 fixtures 静态壳"迁移为"类型安全 + 可降级"的正式 API 客户端，以已生成的 `schema.d.ts` 为唯一类型事实源。

---

## 0. 文档定位

本文聚焦"前端如何接线"——即 SvelteKit 页面从本地 fixtures 切到真实后端 HTTP 调用的一整套实施细节，涵盖：

- 基础设施层（client、信封拆包、错误归一、鉴权、SSE/WS 边界）
- 类型衔接层（`schema.d.ts` 自动类型 vs `types/models.ts` 手写类型）
- 页面接入层（分阶段依赖有序的接线顺序）
- 降级/开发模式（fixtures 的新角色）
- 验收清单与风险处理

本文**不重新设计**后端路由（已 452 条、11 域定型）、不重设计 OpenAPI codegen 管线（已 `tools/openapi-codegen` 独立包落地）、不重写 SvelteKit 页面结构（已静态壳完成）。

---

## 1. 现状事实（接线前的基线）

### 1.1 后端契约

| 维度 | 事实 |
|---|---|
| 路由总数 | **452 operations**（GET 299 / POST 110 / PUT 12 / PATCH 2 / DELETE 29） |
| 域分布（Top 6） | executions 85 · agent-loops 64 · workflows 45 · templates 33 · llm 26 · file-checkpoint 18 |
| 响应信封 | 统一 `ApiEnvelope { success, data, error }`；列表套 `PageView`/`CappedView`；错误 `ErrorResponse` |
| 200 data 类型化 | 约 86% 已具具体类型（阶段 B/C 部分完成）；约 14% 仍为 `ApiEnvelope_Value`（`data: unknown`） |
| 特殊端点 | 5 个 SSE（`text/event-stream`）、1 个 CSV 导出、1 个 Prometheus `/metrics` |
| WebSocket | `crates/app/wf-server/src/ws.rs`，**不在 OpenAPI 中描述** |
| 鉴权 | API key（默认头 `x-api-key`，或查询参数 `api_key`）；由 `AUTH_ENABLED`/`API_KEYS` 控制 |
| 部署 | 同源 `--static-dir` SPA fallback 为推荐生产形态；CORS 由 `cors.toml` 控制 |

### 1.2 前端现状

| 维度 | 事实 |
|---|---|
| 技术栈 | SvelteKit 2 + Svelte 5 + TypeScript 5.9 + Vite 8 |
| 运行时依赖 | **仅 `tailwindcss`**；无 fetch 封装、无 HTTP 客户端库、无验证库 |
| 类型资产 | `src/lib/api/schema.d.ts`（35626 行，openapi-typescript 7 生成）—— **REST 契约唯一事实源** |
| 手写类型 | `src/lib/types/models.ts`（369 行，14 个 ViewModel）—— 目前只被 fixtures 消费 |
| 数据层 | `src/lib/fixtures/`（7 个文件，2200+ 行 mock 数据）—— **所有 10 个页面直接 import** |
| 路由与页面 | 10 个一级页面 + 2 个详情子路由，每个页面内 `<script>` 里直接 import fixtures |
| 无 `+page.ts` | 所有路由无 `load()`，数据在组件层同步消费 fixtures |

### 1.3 页面 → 后端域映射清单

| 前端路由 | 当前数据来源 | 后端主要域 | 关键端点 |
|---|---|---|---|
| `workflows/+page` | `fixtures/workflows.ts` | workflow | `GET /workflows`, `GET /workflows/summaries`, `GET /workflows/search` |
| `workflows/[id]/+page` | `fixtures/workflows.ts` | workflow + workflow/graph | `GET /workflows/{id}`, `GET /workflows/{id}/graph`, `GET /workflows/{id}/versions` |
| `executions/+page` | `fixtures/executions.ts` | workflow/executions | `GET /executions` |
| `executions/[id]/+page` | `fixtures/executions.ts` | workflow/executions + agent/graph + checkpoint + error | **约 40 个子端点**（nodes, graph, performance, state-records, error-analysis, audit/* 等） |
| `agent-loops/+page` | `fixtures/agentLoops.ts` | agent/loops | `GET /agent-loops`, `GET /agent-loops/summaries`, `GET /agent-loops/stats` |
| `agent-loops/[id]/+page` | `fixtures/agentLoops.ts` | agent/loops + agent/graph + checkpoint + variables | **约 20 个子端点**（graph, timeline, conversation, variables, checkpoints 等） |
| `checkpoints/+page` | `fixtures/checkpoints.ts` | checkpoint + file-checkpoint | `GET /checkpoints`, `GET /file-checkpoint/sessions`, `GET /file-checkpoint/tree/{id}`, `GET /file-checkpoint/changes`, `GET /file-checkpoint/approvals/pending` |
| `events/+page` | `fixtures/insights.ts`（EventRecord） | system/events | `GET /events`, `GET /events/stats`, `GET /events/search`, `GET /events/stream`(SSE) |
| `insights/+page` | `fixtures/insights.ts` | query + analysis + executions/audit | `POST /query`, `POST /query/aggregate`, `GET /analysis/stats`, `GET /analysis/performance/compare` |
| `resources/+page` | `fixtures/resources.ts` | llm + scripts + skills + tools + tool-registry | `GET /llm/providers`, `GET /llm/profiles`, `GET /scripts`, `GET /skills`, `GET /tools`, `GET /tool-registry` |
| `triggers/+page` | `fixtures/triggers.ts` | trigger-executions + hooks + triggers | `GET /trigger-executions`, `GET /trigger-executions/stats`, `POST /hooks/{name}`（写入） |
| `templates/+page` | `fixtures/insights.ts`（Template） | template | `GET /templates/agent/summaries`, `GET /templates/library`, `GET /templates/node` |
| `settings/+page` | 无（UI 占位） | web/preferences | `GET /preferences`, `PUT /preferences/{id}` |

---

## 2. 设计目标与非目标

### 2.1 目标

- **类型安全端到端**：请求参数、返回值、错误分支全由 `schema.d.ts` 驱动；禁止手写响应模型。
- **调用点少样板**：一个 `createClient<paths>()` 加薄封装，处理信封拆包、错误归一、鉴权头注入。
- **可降级**：开发期/后端不可用时，能一键回退到 fixtures，页面骨架完整可浏览。
- **接线后能跑**：每个阶段完成后，对应页面能真实渲染后端数据（或明确降级）。
- **边界清晰**：REST / SSE / WebSocket / 下载四类通道各自独立，不混为一谈。

### 2.2 非目标

- 不实现登录/会话（当前后端只有 API key；后续若有 auth 端点再接入）。
- 不实现写操作（`POST`/`PUT`/`PATCH`/`DELETE`）的 UI——本阶段只打通 **只读**；写操作的 client 接口照样生成，但页面不挂按钮。
- 不实现后端还未返回具体 `data` 类型的端点的精确消费（仍返回 `unknown` 时页面按"降级骨架"渲染，直到后端阶段 B/C 补齐）。
- 不在本方案内处理 WebSocket（后端 `ws.rs` 不在 OpenAPI 里；后续单独立 `src/lib/api/ws.ts`）。

---

## 3. 基础设施层设计

### 3.1 新增依赖

```jsonc
"dependencies": {
  // 现有
  "tailwindcss": "^4.1.18",
  // 新增——openapi-fetch 零运行时，仅 peer typescript
  "openapi-fetch": "^0.14.0"
}
```

选 `openapi-fetch` 而非 `@tanstack/query` / `swr`：本项目 SvelteKit 以 `+page.ts` 的 `load()` 为首选数据入口（SSR/预渲染友好），不需要客户端缓存库的复杂度。若后续出现高频轮询场景，再按需引入。

### 3.2 文件结构

```
src/lib/api/
├── schema.d.ts              ← 已存在（openapi-typescript 生成，唯一正式类型）
├── client.ts                ← 新增：createClient + 鉴权 + 错误归一
├── envelope.ts              ← 新增：信封拆包 helper
├── sse.ts                   ← 新增：SSE 客户端（不走 openapi-fetch）
├── errors.ts                ← 新增：可分支的前端错误类型
├── fallback.ts              ← 新增：开发期 fixtures 降级开关
└── domain/                  ← 新增：薄封装（可选，按需要逐域加）
    ├── workflow.ts
    ├── execution.ts
    ├── agent.ts
    └── ...
```

**domain 层薄封装是可选的**——不是"为每个域必须写一个文件"。只有当同一端点在多个页面复用、或需要组合多个端点（如"先拉 summaries 再拉 detail"）时才提取；否则直接在 `+page.ts` 里用 `client.GET('/api/v1/workflows/summaries')` 即可。

### 3.3 `client.ts`——统一入口

```typescript
import createClient from 'openapi-fetch';
import type { paths } from './schema';
import { normalizeError, type ApiError } from './errors';

function resolveBaseUrl(): string {
    // 同源相对路径；生产由 --static-dir 托管后端自动代理；开发靠 vite proxy
    return import.meta.env.VITE_API_BASE ?? '/api/v1';
}

function resolveApiKey(): string | undefined {
    // 优先级：VITE_API_KEY（.env.local）> dev-only demo key > undefined（后端未启鉴权时通）
    if (import.meta.env.VITE_API_KEY) return import.meta.env.VITE_API_KEY;
    if (import.meta.env.DEV) return undefined; // dev 无 key 也可通（默认 AUTH_ENABLED=false）
    return undefined;
}

export const client = createClient<paths>({
    baseUrl: resolveBaseUrl(),
    headers: {
        ...(resolveApiKey() ? { 'x-api-key': resolveApiKey()! } : {}),
        'content-type': 'application/json',
    },
});

// 在响应拦截器里统一处理 401→清 key / 429→toast / 5xx→重试策略
client.use({
    onResponse({ response }) {
        if (response.status === 401) {
            // 触发全局 auth 失效 toast（store 里挂）
        }
        // openapi-fetch 的 throwOnError 默认 false，我们在 envelope 层处理
    },
});
```

**vite 开发代理**（`vite.config.ts`）：同源 `--static-dir` 部署不需要；但后端分端口开发时需加代理避免 CORS：

```typescript
export default defineConfig({
    plugins: [tailwindcss(), sveltekit()],
    server: {
        proxy: {
            '/api': { target: 'http://localhost:3001', changeOrigin: true },
            '/metrics': { target: 'http://localhost:3001', changeOrigin: true },
        },
    },
});
```

### 3.4 `envelope.ts`——信封拆包

后端 HTTP 层统一包了一层 `ApiEnvelope`，`openapi-fetch` 生成的类型已经表达了这一层（`components["schemas"]["ApiEnvelope_PageView_Value"]`），但每次调用点写 `response.data?.data?.items` 非常啰嗦。封装后调用点只需 `.data`：

```typescript
import type { ApiError } from './errors';
import { fallbackEnabled } from './fallback';

/** 信封拆包：
 *  - HTTP 非 2xx → 抛 ApiError
 *  - success=false → 抛 ApiError（带后端 error.code / error.message）
 *  - 否则返回 envelope.data（类型由 schema.d.ts 自动推导）
 *
 * openapi-fetch 的 response.data 已是 application/json 解析结果；
 * 若后端返回的就是信封，response.data.envelope 或 response.data 即信封。
 */
export function unwrap<T>(response: { data?: T; error?: unknown }): T {
    if (response.error) {
        throw normalizeError(response.error);
    }
    // 信封层：response.data 可能是 envelope（{success,data,error}）也可能直接是值
    // 取决于 openapi-typescript 的生成——根据实际 schema.d.ts 确认
    const envelope = response.data as any;
    if (envelope && typeof envelope.success === 'boolean') {
        if (!envelope.success) {
            throw normalizeError(envelope.error);
        }
        return envelope.data as T;
    }
    return response.data as T;
}

/** 分页列表拆包：unwrap 后再取 PageView 的 items/has_more。 */
export function unwrapPage<T>(response: { data?: unknown; error?: unknown }): {
    items: T[];
    limit?: number;
    offset?: number;
    hasMore: boolean;
    total?: number;
} {
    const envelopeData = unwrap(response);
    if (envelopeData && typeof envelopeData === 'object' && 'items' in (envelopeData as any)) {
        const pv = envelopeData as any;
        return {
            items: pv.items ?? [],
            limit: pv.limit,
            offset: pv.offset,
            hasMore: pv.has_more ?? pv.hasMore ?? false,
            total: pv.total,
        };
    }
    // capped 列表
    if (envelopeData && typeof envelopeData === 'object' && 'paths' in (envelopeData as any)) {
        const cp = envelopeData as any;
        return { items: cp.paths ?? [], hasMore: !cp.truncated, total: cp.total };
    }
    return { items: (envelopeData as T[]) ?? [], hasMore: false };
}
```

### 3.5 `errors.ts`——可分支错误

```typescript
export type ApiErrorCode =
    | 'UNAUTHORIZED'    // 401
    | 'FORBIDDEN'       // 403
    | 'RATE_LIMITED'    // 429
    | 'NOT_FOUND'       // 404
    | 'INVALID_PARAMS'  // 400
    | 'SERVER_ERROR'    // 5xx
    | 'NETWORK_ERROR'   // fetch 失败
    | 'PARSE_ERROR';    // 响应体非法 JSON

export class ApiError extends Error {
    readonly code: ApiErrorCode;
    readonly httpStatus?: number;
    constructor(code: ApiErrorCode, message: string, httpStatus?: number) {
        super(message);
        this.code = code;
        this.httpStatus = httpStatus;
    }
}

export function normalizeError(raw: unknown): ApiError {
    if (raw instanceof ApiError) return raw;
    if (raw && typeof raw === 'object') {
        const obj = raw as Record<string, unknown>;
        // envelope.error 或 ErrorResponse
        const code = obj.code as string | undefined;
        const message = (obj.message ?? obj.error ?? obj.success) as string | undefined;
        return new ApiError(mapCode(code), message ?? 'Unknown API error');
    }
    if (typeof raw === 'string') return new ApiError('SERVER_ERROR', raw);
    return new ApiError('SERVER_ERROR', 'Unknown error');
}

function mapCode(code?: string): ApiErrorCode {
    if (!code) return 'SERVER_ERROR';
    return (CODE_MAP as Record<string, ApiErrorCode>)[code] ?? 'SERVER_ERROR';
}

const CODE_MAP: Record<string, ApiErrorCode> = {
    unauthorized: 'UNAUTHORIZED',
    forbidden: 'FORBIDDEN',
    not_found: 'NOT_FOUND',
    invalid_params: 'INVALID_PARAMS',
    rate_limited: 'RATE_LIMITED',
};
```

### 3.6 `fallback.ts`——降级开关

**核心决策**：fixtures **不删，不迁到 mock server**，而是在 `+page.ts` 的 `load()` 里按开关切换数据来源。理由：

- MSW / mswjs.io 在 SvelteKit SSR 场景下需要 Node 拦截层，配置复杂且与 Svelte 5 runes 共存时已知有坑。
- 降级场景是"前端独立演示"或"后端起不来"——fixtures 直出即可，不需要真正的 HTTP 拦截。
- 开关粒度 = 全局一个布尔，而非按端点注册（后者维护成本高）。

```typescript
/**
 * 降级模式：
 *  - DISABLED: 强制走真实 API，失败抛异常
 *  - ENABLED:  load() 内 try 真实 API，catch 或 env 开启则返回 fixtures
 *
 * 优先级：import.meta.env.VITE_API_FALLBACK === 'true' > dev 下后端连不上时自动降级
 */
export function fallbackEnabled(): boolean {
    if (import.meta.env.VITE_API_FALLBACK === 'true') return true;
    return false;
}
```

**fixtures 的新角色**：从"被组件直接 import"变为"被 `+page.ts` 的 fallback 分支 import"。组件只消费 `load()` 传入的数据，不关心来源。

### 3.7 `sse.ts`——流式端点

openapi-typescript 只知道 `text/event-stream` 的 content-type，无法描述事件帧类型。SSE 独立实现：

```typescript
export interface SseFrame<T> {
    event?: string;
    data: T;      // 按后端事件枚举解析
    id?: string;
    retry?: number;
}

export function subscribeSse<T>(url: string, options?: {
    apiKey?: string;
    onMessage: (frame: SseFrame<T>) => void;
    onError?: (err: unknown) => void;
    onOpen?: () => void;
}): () => void {
    // 用原生 EventSource（不能带自定义头 → 用 query param api_key）
    const apiKey = options?.apiKey ?? resolveApiKey();
    const fullUrl = apiKey ? `${url}?api_key=${encodeURIComponent(apiKey)}` : url;
    const es = new EventSource(fullUrl);
    es.onopen = options?.onOpen ?? (() => {});
    es.onerror = (e) => options?.onError?.(e);
    es.onmessage = (ev) => {
        try {
            const data = JSON.parse(ev.data) as T;
            options?.onMessage({ data, event: ev.event, id: ev.lastEventId });
        } catch {
            options?.onMessage({ data: ev.data as unknown as T });
        }
    };
    return () => es.close();
}
```

SSE 事件帧的 `T` 类型从后端 `events` 枚举手写镜像（后端 OpenAPI 文档若补了事件 payload schema 再切换引用）。

---

## 4. 类型衔接策略

### 4.1 问题

`schema.d.ts` 与 `types/models.ts` 同时存在，前者自动生成、后者手写。两者字段**不保证**一致（后端演进时手写类型容易漂移）。页面组件消费的是 `models.ts` 的类型（通过 fixtures），切到真实 API 后返回的是 `schema.d.ts` 的类型。

### 4.2 结论：**adapter 层而非二选一**

三个选项对比：

| 方案 | 优点 | 缺点 |
|---|---|---|
| A. 删掉 models.ts，组件全用 schema.d.ts | 单一事实源，零漂移 | schema 类型按 endpoint 粒度生成（`components["schemas"]["ApiEnvelope_..."]`），组件 props 想用跨端点复用的"纯 Execution"类型时要从 schema 里拆；大量组件 props 签名要改 |
| B. 保留 models.ts，API 返回后做字段映射 adapter | 组件无感知，渐进迁移 | 维护两套类型；后端加字段时 adapter 要同步补；漂移风险转移到 adapter |
| C. 生成模型层（如 `openapi-typescript` 的 `-o` + `services`） | 自动 | openapi-typescript 7 不支持多文件产物；且仍需 adapter |

**选 B（adapter 层）**，理由：

1. 现有 10 个页面组件 props 已写死用 `Execution`、`Workflow`、`AgentLoop` 等模型类型，改动成本不可忽视。
2. 后端 schema 里的命名是 `components["schemas"]["ApiEnvelope_PageView_Value"]`，没有一个统一的 `Execution` 接口——endpoint 响应结构各异（有的带字段 A、有的带 B）。手写的 `Execution` 反而是**跨端点聚合后的视图模型**，更贴近组件真实需求。
3. 漂移风险可收敛：adapter 集中在 `src/lib/api/adapters/` 下，每个域一个文件，测试覆盖每个端点 → 模型的映射。后端阶段 B/C 继续推进时，adapter 跟着更新。
4. 当某页面只需"直接透传"（例如只展示某单一字段），可以**跳过 adapter 直接用 schema 类型**——不强求全走 adapter。

### 4.3 adapter 层示例

```typescript
// src/lib/api/adapters/executions.ts
import type { paths } from '../schema';
import type { Execution } from '$lib/types/models';

type SummariesResp = paths['/api/v1/executions']['get']['responses']['200']['content']['application/json'];
// → 推导出 envelope.data 的精确类型

export function adaptExecutionSummary(raw: SummariesResp): Execution {
    const pv = raw.data as any; // unwrap 之后是 PageView 实例
    const item = pv.items[0]; // 示意
    return {
        id: item.id,
        workflowId: item.workflow_id,
        workflowName: item.workflow_name ?? item.workflow,
        status: item.status,
        startedAt: item.started_at ?? item.startedAt,
        endedAt: item.ended_at ?? item.endedAt ?? null,
        // ...
    };
}
```

### 4.4 何时去掉 adapter

当后端阶段 C1（镜像 DTO）或 C2（下沉 derive）完成，且某个 endpoint 的 `data` 类型字段已与前端 ViewModel 一致时，直接去掉 adapter，组件消费 schema 类型。**长期目标**是 adapter 层全部消失，但节奏由后端类型化进度决定。

### 4.5 `types/models.ts` 的角色变化

- 不再被 fixtures 直接 import（fixtures 也走 adapter：`adaptExecutionSummary({data: fixtureRaw})`）。
- 成为**组件 props 的消费契约**，其字段是"前端视图真正需要的聚合结构"。
- 每个类型文件头部加 `// Adapter contract —— 字段来源见 src/lib/api/adapters/<domain>.ts` 注释，避免开发者在 models.ts 里手填后端原始字段。

---

## 5. 页面接入：分阶段顺序

排序依据：**域依赖关系 + 页面独立度 + 后端路由数量**。让每个阶段完成后都有**可运行、可验收的页面**，而不是一堆半成品。

### 阶段 0：基础设施 + 一个页面打通（前置 Spike）

**目标**：让 `agent-loops/+page`（列表）真正跑起来，端到端验证 client + envelope + adapter + fallback 整条链路。

| 任务 | 产物 |
|---|---|
| 新增 `openapi-fetch` 依赖 | `package.json` |
| 创建 `client.ts` + `envelope.ts` + `errors.ts` + `fallback.ts` | `src/lib/api/*.ts` |
| `vite.config.ts` 加 dev proxy | `vite.config.ts` |
| `+layout.ts` 注入全局 toast store 供 401/429 消费 | `src/routes/+layout.ts` |
| `src/lib/api/adapters/agent.ts`（agent loop summary） | adapter |
| 创建 `agent-loops/+page.ts`，调用 `client.GET('/api/v1/agent-loops/summaries')` + `unwrapPage()` + adapter | `+page.ts` |
| `agent-loops/+page.svelte` 从 `import { agentLoops } from '$lib/fixtures/agentLoops'` 改为消费 `$page.data.agentLoops` | 组件最小改动 |
| 验证：真实后端跑起来页面渲染、`svelte-check` 通过、fallback 开关切到 ENABLED 页面仍可用 | 验收 |

**为什么选 agent-loops/+page**：`/agent-loops/summaries` 端点返回实时 + 持久化合并的汇总，对 UI 最友好；数据字段在 mock 和后端之间结构最接近；schema.d.ts 里已是 `ApiEnvelope_CappedPaths`（已类型化）。

### 阶段 1：核心列表页（Workflow + Execution）

两个最高频页面，数据量最大。先列表后详情。

| 页面 | 后端端点 | 关键 adapter |
|---|---|---|
| `workflows/+page` | `GET /workflows/summaries`, `GET /workflows` | `WorkflowSummary` |
| `executions/+page` | `GET /executions`（带 query: status/workflow_id） | `ExecutionSummary` |

**增量工作**：
- `+page.ts` 加 2-3 条 `client.GET` 调用
- adapter 层加 2 个 summary 函数
- 组件数据入口替换：`{summaryRows}` → `$page.data.summaryRows`

### 阶段 2：Agent Loops 详情页 + Workflow / Execution 详情页

详情页数据量大，需要拆多次调用（主信息 + 子端点）。以 `executions/[id]` 为例：

```typescript
// src/routes/executions/[id]/+page.ts
export async function load({ params }) {
    const id = params.id;
    const [execResp, nodesResp, graphResp] = await Promise.all([
        client.GET('/api/v1/executions/{id}', { params: { path: { id } } }),
        client.GET('/api/v1/executions/{id}/nodes', { params: { path: { id } } }),
        client.GET('/api/v1/executions/{id}/graph', { params: { path: { id } } }),
    ]);
    return {
        execution: unwrap(execResp).then(adaptExecutionDetail),
        nodes: unwrap(nodesResp).then(adaptNodes),
        graph: unwrap(graphResp).then(adaptGraph),
    };
}
```

如果后续出现"先加载概要、详情按需加载"的需求，用 `load` + `depends` 或 `pageStore.set` 拆分即可。

**此阶段同时覆盖**：`agent-loops/[id]`、`workflows/[id]`。每个详情页约需要 5-15 个子端点（取决于 UI 展示哪些 Tab/Panel）——**按实际组件消费的字段选择**，不追求一次把所有子端点都接上。

### 阶段 3：其余列表页 + Settings

| 页面 | 域 | 说明 |
|---|---|---|
| `checkpoints/+page` | checkpoint + file-checkpoint | 三个 Tab：链路/文件工作区/审批，各需独立端点 |
| `triggers/+page` | trigger-executions + hooks | 列表 + 历史 + 统计 |
| `resources/+page` | llm + scripts + skills + tools + tool-registry | 跨 5 个域的聚合页 |
| `templates/+page` | template + template/library | agent/workflow/node/trigger 四类模板 |
| `settings/+page` | web/preferences | 目前纯 UI 占位，本次一起接上 |

### 阶段 4：Insights + Events（查询分析页）

这两类页面特点是"执行查询 → 渲染结果"，不是简单列表：

- `events/+page`：`GET /events`（分页）+ `GET /events/search`（带 query）+ **SSE** `/events/stream`（实时推送，页面已有的 Stream/Timeline 组件要接）
- `insights/+page`：`POST /query`（自定义 SQL 等价的表达式）+ `POST /query/aggregate`（统计分面）+ `GET /analysis/stats` + `GET /analysis/performance/compare`

**SSE 首次接入**：`events/stream` 走 `subscribeSse<T>()`，T 从后端 EventType 枚举镜像（先手写，后端阶段 C 再切引用）。Svelte 5 runes 下用 `$state` 数组累积事件帧。

### 阶段 5：写操作 + WebSocket（留待后续）

- 写操作 UI（run、pause、cancel、approve、trigger fire、template import 等）在后端类型化稳定后再挂按钮。
- WebSocket（`ws.rs`）不在 OpenAPI 中描述，单独开 `src/lib/api/ws.ts`，协议类型手写。

---

## 6. `+page.ts` 模式与组件改造模式

### 6.1 一个典型的 `+page.ts`（阶段 1 示例）

```typescript
// src/routes/workflows/+page.ts
import type { PageData } from './$types';
import { client } from '$lib/api/client';
import { unwrapPage } from '$lib/api/envelope';
import { fallbackEnabled } from '$lib/api/fallback';
import { adaptWorkflowSummary } from '$lib/api/adapters/workflow';
import { workflows as fixtureWorkflows } from '$lib/fixtures/workflows';

export async function load({ fetch }): Promise<PageData> {
    if (fallbackEnabled()) {
        return { workflows: fixtureWorkflows };
    }
    try {
        const resp = await client.GET('/api/v1/workflows/summaries', {
            fetch, // SvelteKit SSR 环境下传 load 的 fetch
        });
        const page = unwrapPage(resp);
        const workflows = page.items.map(adaptWorkflowSummary);
        return { workflows };
    } catch (err) {
        if (import.meta.env.DEV) {
            console.warn('[fallback] workflows endpoint failed, using fixtures', err);
            return { workflows: fixtureWorkflows };
        }
        throw err;
    }
}
```

**关键细节**：`client.GET` 必须传 `fetch` 参数——在 SvelteKit SSR 环境下它会自动走 SvelteKit 的请求上下文（携带 cookies / 跟随重定向）。client 内部在 SSR 时用 `load.fetch`、CSR 时自动用 `globalThis.fetch`。

### 6.2 组件改造模板

```diff
 <!-- +page.svelte -->
 <script lang="ts">
+    import type { PageData } from './$types';
+    export let data: PageData;
-    import { workflows } from '$lib/fixtures/workflows';
-    import type { Workflow } from '$lib/types/models';
+    let { workflows } = $derived(data);
 </script>
```

**改造策略**：
- 把 `import fixtures` 替换为消费 `$page.data`（SvelteKit 的页面 store）。
- 如果当前组件把数据作为 prop 传给子组件（如 `ExecutionCard`、`WorkflowCard`），只需保持 prop 类型不变——adapter 输出的正是同类型。
- 组件内的"刷新"按钮：加一个 `onclick` handler，内部调用 `invalidateAll()` 或定向 `invalidate('/workflows')`，不自己重写 client 调用。

---

## 7. schema.d.ts 的生命周期

### 7.1 何时刷新

触发条件：后端 `crates/app/wf-server/src/api/**` 下任何 handler 的 `#[utoipa::path(...)]` 注解、响应体类型、全局 securitySchemes 或 components 注册发生变更时。

### 7.2 刷新流程

```shell
# 1. 后端刷新 golden-file
WF_REFRESH_OPENAPI=1 cargo test -p wf-server committed_snapshot_matches_document
# 产物：apps/web-app/openapi.json 更新

# 2. 前端 codegen（独立包，TS5）
cd tools/openapi-codegen
npm install          # 首次或 openapi-typescript 升级时
npm run gen          # 读 ../../apps/web-app/openapi.json，写 schema.d.ts 到当前目录

# 3. 复制到 web-app
cp schema.d.ts ../../apps/web-app/src/lib/api/schema.d.ts

# 4. 验证
cd ../../apps/web-app
npm run check        # svelte-check
```

### 7.3 漂移检测

- CI 加 `WF_REFRESH_OPENAPI=1 cargo test -p wf-server committed_snapshot_matches_document`（已存在）。
- CI 再加 `cd tools/openapi-codegen && npm ci && npm run gen && git diff --exit-code apps/web-app/src/lib/api/schema.d.ts`——任何注解变更未同步刷新 `.d.ts` 直接变红。
- 漂移时修复流程：按 7.2 执行即可。

### 7.4 当前 schema.d.ts 的已知缺陷

- `ApiEnvelope_Value`（`data: unknown`）出现在约 14% 的端点——这些端点在前端接线时，adapter 接收 `unknown`，返回的 ViewModel 只能覆盖已知字段。后端阶段 B/C 补齐类型后 adapter 会自动获得更精确的输入类型（TypeScript 推断），可以去掉运行时 `as any`。

---

## 8. 验收清单

### 8.1 基础设施（阶段 0 完成后）

- [ ] `npm install openapi-fetch` 完成；`package.json` 依赖声明正确
- [ ] `openapi-fetch` 未拉入 `typescript` peer 冲突（web-app TS 5.9，openapi-fetch 0.14 支持）
- [ ] `npm run check`（svelte-check）通过
- [ ] 全局搜索 `openapi-typescript`：web-app `package.json` 不应出现（codegen 工具在独立包）
- [ ] `src/lib/api/schema.d.ts` 的 `// Do not make direct changes` 注释仍存在

### 8.2 阶段 0（agent-loops 列表）

- [ ] `agent-loops/+page` 跑真实后端时渲染数据（CURL 验证 `/api/v1/agent-loops/summaries` 与 UI 渲染一致）
- [ ] `VITE_API_FALLBACK=true npm run dev` 时渲染 fixtures
- [ ] 后端返回 401 时显示 toast 错误，不 crash
- [ ] 后端返回 500 时 error 边界 toast 有内容（来自 `error.message`），不显示空白骨架
- [ ] 刷新按钮触发 `invalidate('/agent-loops')` 且 1 秒内新数据到达

### 8.3 阶段 1（核心列表）

- [ ] `workflows/+page` 与 `executions/+page` 真实数据渲染正确
- [ ] 分页按钮/加载更多走 `unwrapPage` 的 `hasMore`，不是硬编码
- [ ] adapter 层每个函数有单元测试（vitest，mock `client` 返回值）

### 8.4 阶段 2（详情页）

- [ ] `executions/[id]` 至少打通 3 个子端点（主信息 / nodes / graph）
- [ ] 详情页"返回列表"链路不丢数据、不 crash
- [ ] SvelteKit 的 `depends(...)` 声明在多 endpoint 并行加载时正确标记

### 8.5 阶段 4（SSE）

- [ ] `events/stream` 帧类型正确解析（至少区分事件类型 + 携带 data）
- [ ] EventSource 断线自动重连（原生支持，verify onopen 触发次数 ≤ 预期）
- [ ] SSE fallback：后端连不上或 content-type 不对时，events 页面走 `GET /events` 并显示"实时流不可用"提示

### 8.6 类型与漂移

- [ ] CI 漂移检测脚本跑通（7.3 节）
- [ ] adapter 文件每个函数的输入类型从 schema.d.ts 推导（`pnpm exec tsc --noEmit` 报告 adapter 里没有隐式 any 除了在 `// 已知 backend 未类型化` 标记处）
- [ ] 无直接 import `src/lib/fixtures/*` 的 `.svelte` 组件（全部从 `+page.ts` 传入）

---

## 9. 风险与处理

| 风险 | 概率 | 影响 | 处理 |
|---|---|---|---|
| `openapi-fetch` 与 SvelteKit SSR `fetch` 参数传法不对，导致 CSR/SSR 行为不一致 | 中 | 页面前端能跑、SSR 报错 | 阶段 0 里特意验证 SSR（`npm run build && npm run preview`）；`client.GET` 必传 `{ fetch }` |
| 后端阶段 C 推进时 schema.d.ts 刷新导致 adapter 输入类型突变（字段名改了/类型缩窄） | 中 | TS 报错，adapter 要同步改 | adapter 每个文件顶部加 `// Last schema.d.ts sync: YYYY-MM-DD`，CI 漂移检测变红时强制同步 |
| `ApiEnvelope_Value` 的 14% 端点 adapter 接收 unknown，字段访问全是 `as any` | 高 | 组件拿到 undefined 时 crash | adapter 输出必走 `?? null` 防御式取值；组件对 null 展示空态（EmptyState 组件已落地） |
| vite 开发代理 `/api/v1` 前缀与同源 `--static-dir` 生产部署行为不一致 | 低 | dev 能跑、prod 路径 404 | 部署文档里明确 dev 用 vite proxy、prod 用同源托管；`baseUrl` 默认写 `/api/v1`，两边一致 |
| 组件 `Execution` / `Workflow` 类型字段名与后端 snake_case 不一致 | 高 | adapter 工作量大 | adapter 函数体做 camelCase 转换（`backend_field` → `backendField`）；集中在 adapter 一层，组件 props 保持 camelCase |
| fixtures 被完全替换后，前端独立演示/截图/离线开发场景丢失 | 中 | 产品侧不方便演示 | `fallback.ts` 永久保留；`VITE_API_FALLBACK=true` 构建独立演示产物；fixtures 文件保留不删 |

---

## 10. 未决事项（需要用户决策）

1. **写操作 UI 何时接入**：当前方案只覆盖只读。如果写操作（执行、暂停、审批、触发）优先级高，可以把阶段 5 提前到阶段 3 之后。
2. **settings 页接 `PUT /preferences/{id}` 后是否需要立即实现"持久化到后端"还是暂时只走 localStorage**：后者改动小，可以作为 settings 页的 fallback。
3. **WebSocket（`ws.rs`）是否在本周期接线**：后端 `ws::routes()` 已经注册但 OpenAPI 未覆盖，协议待手写。如果要本周期做，需要另开 `src/lib/api/ws.ts`，工作量约 0.5-1 个阶段。

---

## 11. 实施速查卡

```
[依赖]        npm install openapi-fetch
[基础设施]    src/lib/api/{client,envelope,errors,fallback,sse}.ts
[adapter]     src/lib/api/adapters/{workflow,execution,agent,checkpoint,event,...}.ts
[刷新 codegen] cd tools/openapi-codegen && npm run gen && cp schema.d.ts ../../apps/web-app/src/lib/api/
[刷新后端 JSON] WF_REFRESH_OPENAPI=1 cargo test -p wf-server committed_snapshot_matches_document
[验证]        npm run check && npm run build
```

**阶段顺序**：0 spike → 1 core lists → 2 detail pages → 3 remaining lists + settings → 4 insights + SSE → 5 writes + ws
