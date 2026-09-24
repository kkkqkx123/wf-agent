# Web-App 前端 API 正式接线设计方案

> **范围**：`apps/web-app` 所有页面从 `$lib/fixtures/*` 硬编码数据迁移到 `wf-server` 后端真实 API。
> **约束**：后端已完整（`crates/app/wf-api` + `crates/app/wf-server`），**绝对禁止任何假数据、硬编码数据、绕过后端的本地数据**。所有数据必须实际来自后端 HTTP API。
> **配套**：`docs/plan/web-app-integration.md`（客户端技术选型）、`docs/plan/web/server-gaps.md`（后端已确认无阻塞缺口）。

---

## 0. 现状事实（作为设计输入）

### 0.1 前端技术栈

| 项目 | 版本 |
|------|------|
| SvelteKit | 2.69 |
| Svelte | 5.56（$state / $derived runes） |
| TypeScript | 5.9（由 TS7 svelte-check 消费） |
| Vite | 8.1 |
| Tailwind CSS | 4.1 |

SSR = false（`+layout.ts` 中明确声明），SPA 纯客户端。

### 0.2 后端 API 契约（已由 OpenAPI 快照锁定）

- 路由总数：**约 452 条 operation**（`schema.d.ts` 行数 41,309）
- 统一前缀：`/api/v1`（业务域），根路径放 `/health` `/info` `/api-docs/openapi.json`
- **响应信封**：`ApiEnvelope<T> { success, data, error }`
- **列表分页**：`PageView<T> { items, has_more, limit, offset }`（默认 50，上限 500）
- **封顶列表**：`CappedView<T> { items, total, truncated }`（大链/时间线专用）
- **错误体**：`ApiErrorBody { code: string, message: string }`
- **鉴权**：API Key，头 `x-api-key` 或查询参数 `api_key`，`AUTH_ENABLED` 开关
- **CORS**：由 `cors.toml` 配置，前端需在同源或经代理部署
- **流式通道**：`/api/v1/events/stream`（SSE）、`/ws`（WebSocket）
- **无分页的静态返回**：`health`、`info`、`dependencies/impact`、少量统计——直接裸 JSON

### 0.3 前端当前数据来源（全部为假数据）

| 文件 | 导出名 | 覆盖页面 |
|------|--------|----------|
| `fixtures/workflows.ts` | `workflows`, `workflowDetail` | workflows list + detail |
| `fixtures/agentLoops.ts` | `agentLoops`, `loopDetail`, `loopMessages`, `loopVariables` | agent-loops list + detail |
| `fixtures/executions.ts` | `executions`, `executionDetail` | executions list + detail |
| `fixtures/checkpoints.ts` | `checkpoints`, `approvals`, `fileChanges` | checkpoints |
| `fixtures/triggers.ts` | `triggerRecords`, `hooks` | triggers |
| `fixtures/resources.ts` | `modelProfiles`, `providers`, `tools`, `scripts`, `skills` | resources |
| `fixtures/insights.ts` | `overviewMetrics`, `auditReports`, `errorAnalyses`, `perfNodes`, `queryResult`, `templates`, `events`, `dependencies`, `diagnostics` | insights / events / templates |
| `fixtures/clock.ts` | `minutesAgo()` | 辅助函数 |
| `stores/preferences.svelte.ts` | `preferences` | settings（本地持久化，需接后端） |

**所有 10 个路由页面 + 3 个动态 `[id]` 详情页均依赖 fixtures。** settings 页的本地 store 有后端对应通道（`/api/v1/preferences/*`）。

### 0.4 后端路由分组（router.rs 已固定）

| 后端模块 | 对应前端域 | 关键路径前缀 |
|----------|-----------|-------------|
| `api::workflow::{workflows,versions,graphs,executions,execution_state,execution_analysis,approvals,drafts}` | workflows + executions | `/api/v1/workflows/*`, `/api/v1/executions/*`, `/api/v1/graph/*` |
| `api::agent::{profiles,loops,executions,graphs,analysis,variables,drafts}` | agent-loops | `/api/v1/agent-loops/*`, `/api/v1/agents/*`, `/api/v1/agent-executions/*` |
| `api::checkpoint::{checkpoints,file_provenance,file_approvals}` | checkpoints | `/api/v1/checkpoints/*`, `/api/v1/files/*` |
| `api::trigger::{executions,hooks}` | triggers | `/api/v1/trigger-executions/*`, `/api/v1/hooks/*` |
| `api::web::{preferences,favorites,batch}` | settings + 收藏 | `/api/v1/preferences/*`, `/api/v1/favorites/*` |
| `api::llm::{routes,scripts,tools}` | resources（模型/脚本/工具） | `/api/v1/llm/*`, `/api/v1/scripts/*`, `/api/v1/tools/*` |
| `api::template::{templates,queries,library}` | templates | `/api/v1/templates/*` |
| `api::entity::{messages,tasks,variables,skills,interactions}` | resources（技能）+ agent-detail | `/api/v1/messages/*`, `/api/v1/skills/*`, `/api/v1/interactions/*` |
| `api::observation::{query,audit,analysis}` | insights | `/api/v1/query/*`, `/api/v1/audit/*`, `/api/v1/analysis/*` |
| `api::system::{events,dependencies}` | events | `/api/v1/events/*`, `/api/v1/dependencies/*` |
| `ws::routes` | events stream | `/ws` |

---

## 1. 总体方案

### 1.1 三层架构

```
┌─────────────────────────────────────────────────────┐
│  routes/*/+page.svelte（页面组件）                     │
│  └─ 使用 SvelteKit $derived / 组件 props 渲染         │
├─────────────────────────────────────────────────────┤
│  $lib/services/*（后端 API 适配层，本方案新增）        │
│  ├─ 调用 client 拿原始数据                            │
│  ├─ 执行 envelope 拆包、DTO → ViewModel 映射          │
│  ├─ 处理 loading / error / 空状态                    │
│  └─ 提供 SvelteKit load() 或 async fn                │
├─────────────────────────────────────────────────────┤
│  $lib/api/client.ts（类型安全 fetch 客户端）           │
│  ├─ createClient<paths>（openapi-fetch）             │
│  ├─ baseUrl / 鉴权头注入 / 信封拆包                   │
│  ├─ 错误归一                                          │
│  └─ 分页 / 封顶视图统一处理                            │
└─────────────────────────────────────────────────────┘
         ▲ HTTP
         ▼
  wf-server /api/v1
```

### 1.2 数据流

```
页面 mount
  → load() 调用 service 层
    → service 层调用 client
      → client 发请求
        → 后端返回 ApiEnvelope<T>
          → client 拆包 → data（unknown）
            → service 层映射 models.ts ViewModel
              → 页面 $derived 消费
```

### 1.3 fixture 文件处置

- **保留** `fixtures/clock.ts`（`minutesAgo` 辅助函数可删可改，或由 `formatDateTime` 统一替代）
- **删除** 其余 7 个 fixture 文件中所有导出数据；**改造前先确认每个文件的导出名与页面引用位置**。
- 禁止"只替换某个 fixture 文件为 API wrapper"——fixtures 目录语义就是假数据目录，完成接线后该目录清空或仅保留 clock.ts 辅助。

---

## 2. client 层详细设计

### 2.1 依赖

```jsonc
// apps/web-app/package.json
{
  "dependencies": {
    "openapi-fetch": "^0.14.0"
  }
}
```

**理由**：`openapi-fetch` 与 `openapi-typescript` 官方配套，自动消费 `paths` 泛型，零运行时 schema、类型检查在编译期完成。不选用 `openapi-typescript-fetch`（已废弃）或手写 fetch + 类型断言（违反"类型来源唯一"原则）。

### 2.2 client.ts 结构

路径：`apps/web-app/src/lib/api/client.ts`

```typescript
import createClient from 'openapi-fetch';
import type { paths } from './schema';

// 环境配置读取顺序：import.meta.env.VITE_API_BASE_URL → 当前 origin + /api/v1
const BASE_URL = import.meta.env.VITE_API_BASE_URL
  ?? `${location.origin}/api/v1`;

const getApiKey = (): string | undefined => {
  // 来源：import.meta.env.VITE_API_KEY → localStorage['wf.apiKey'] → undefined
  return import.meta.env.VITE_API_KEY
    ?? localStorage.getItem('wf.apiKey')
    ?? undefined;
};

export const client = createClient<paths>({
  baseUrl: BASE_URL,
  headers: { 'Content-Type': 'application/json' },
});

client.use({
  async onRequest({ request }) {
    const key = getApiKey();
    if (key) request.headers.set('x-api-key', key);
    return request;
  },
  async onResponse({ response, request }) {
    // 401 / 403：弹 toast 并可选跳转 settings
    // 429：读取 Retry-After 头提示
    return response;
  },
});
```

### 2.3 信封拆包辅助

路径：`apps/web-app/src/lib/api/envelope.ts`

```typescript
import type { components } from './schema';

type ApiError = components['schemas']['ApiErrorBody'];

export class ApiHttpError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
    this.name = 'ApiHttpError';
  }
}

export async function call<T>(
  promise: Promise<{ data?: unknown; error?: unknown }>,
): Promise<T> {
  const res = await promise;
  if (res.error) {
    // openapi-fetch 在 HTTP 4xx/5xx 时填充 res.error
    // 格式：{ error: { status, headers, body } }
    const e = res.error as any;
    if (typeof e === 'object' && 'body' in e) {
      const body = e.body as ApiError;
      throw new ApiHttpError(e.status ?? 0, body?.code ?? 'UNKNOWN', body?.message ?? e.message);
    }
    throw new Error(String(e));
  }
  // openapi-fetch 成功时 data 字段就是 envelope.data（已拆一层）
  return res.data as T;
}

export interface PageResult<T> {
  items: T[];
  hasMore: boolean;
  limit: number;
  offset: number;
}

export interface CappedResult<T> {
  items: T[];
  total: number;
  truncated: boolean;
}
```

### 2.4 鉴权配置设计

**生产方案**：前端与后端同源部署（`wf-server` 的 `static_dir` 托管前端产物 + SPA 回退），无需跨域鉴权，`x-api-key` 由 `AUTH_ENABLED` 开关决定是否强制。

**开发方案**：`vite.config.ts` 添加 proxy：

```typescript
export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    proxy: {
      '/api': { target: 'http://127.0.0.1:7859', changeOrigin: true },
      '/health': { target: 'http://127.0.0.1:7859', changeOrigin: true },
      '/info': { target: 'http://127.0.0.1:7859', changeOrigin: true },
      '/ws': { target: 'http://127.0.0.1:7859', ws: true, changeOrigin: true },
    },
  },
});
```

API Key 开发期通过 `.env` 文件注入，生产期通过后端配置控制。

### 2.5 流式通道

- **SSE**（`/api/v1/events/stream`）：直接用 `EventSource`，类型化在 service 层处理
- **WebSocket**（`/ws`）：用原生 `WebSocket`，订阅维度由 `opcode` 指定
- **不在 client.ts 中封装**——各自独立的 service 函数处理

---

## 3. Service 层设计

### 3.1 目录结构

```
src/lib/services/
├── index.ts                     ← barrel export
├── workflows.ts                 ← workflows + workflow-detail + graph
├── executions.ts                ← executions list + detail + state + analysis
├── agent-loops.ts               ← agent-loops + messages + variables + graph
├── checkpoints.ts               ← checkpoints + file workspace + approvals
├── triggers.ts                  ← trigger executions + hooks
├── resources.ts                 ← model profiles + providers + tools + scripts + skills
├── insights.ts                  ← query + audit + errors + performance
├── events.ts                    ← event stream + dependencies + diagnostics
├── templates.ts                 ← template library + by-kind + featured/popular
├── settings.ts                  ← preferences CRUD
├── queries.ts                   ← 跨域查询（执行/回路/工作流）
├── sse.ts                       ← EventSource 封装
└── ws.ts                        ← WebSocket 封装
```

### 3.2 DTO → ViewModel 映射策略

**核心原则**：不修改 `models.ts` 现有 ViewModel 结构——它已经是页面消费的稳定接口。Service 层把后端 `unknown` DTO（因 schema.d.ts 中具体 DTO 大多为 `unknown`）映射到 ViewModel。

**映射模式**：

```typescript
// services/workflows.ts 示例
import { client } from '$lib/api/client';
import { call, type PageResult } from '$lib/api/envelope';
import type { Workflow, WorkflowDetail, WorkflowGraph } from '$lib/types/models';

interface WorkflowDto {
  id: string;
  name: string;
  description?: string;
  category?: string;
  tags?: string[];
  author?: string;
  version?: number;
  status?: string;
  node_count?: number;
  edge_count?: number;
  updated_at?: string;
  runs?: number;
  success_rate?: number | null;
}

function toWorkflow(dto: WorkflowDto): Workflow {
  return {
    id: dto.id,
    name: dto.name,
    description: dto.description ?? '',
    category: dto.category ?? '',
    tags: dto.tags ?? [],
    author: dto.author ?? '',
    version: dto.version ?? 1,
    status: dto.status ?? 'active',
    nodeCount: dto.node_count ?? 0,
    edgeCount: dto.edge_count ?? 0,
    updatedAt: dto.updated_at ?? '',
    runs: dto.runs ?? 0,
    successRate: dto.success_rate ?? null,
  };
}

export async function listWorkflows(params?: {
  status?: string;
  limit?: number;
  offset?: number;
}): Promise<PageResult<Workflow>> {
  const data = await call<any>(
    client.GET('/api/v1/workflows', {
      params: { query: params as any },
    }),
  );
  return {
    items: (data?.items ?? []).map(toWorkflow),
    hasMore: data?.has_more ?? false,
    limit: data?.limit ?? 50,
    offset: data?.offset ?? 0,
  };
}
```

**DTO 类型来源**：schema.d.ts 的 components/schemas 里已定义了部分（`RunAgentLoopBody`、`ApiErrorBody` 等），service 层**只对入参 DTO 复用 schema.d.ts 的命名类型**，对响应 DTO 本地定义最小 interface（schema.d.ts 的响应体当前大多是 `unknown`，见 §7 的改进建议）。

### 3.3 后端路径 → service 函数映射（完整清单）

#### 3.3.1 Workflows 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `Workflow[]` | `workflows` | `GET /api/v1/workflows` | `listWorkflows({ status, limit, offset })` |
| `WorkflowDetail` | `workflowDetail` | `GET /api/v1/workflows/{id}` + `GET /api/v1/workflows/{id}/graph` + `GET /api/v1/workflows/{id}/versions` + `GET /api/v1/workflows/{id}/drafts` + `GET /api/v1/workflows/{id}/neighbors` | `getWorkflowDetail(id)` → parallel 5 路聚合 |

**详情页聚合**：页面消费的 `WorkflowDetail` 是一个聚合对象，后端拆成 5 条独立路径。service 层用 `Promise.allSettled` 并行请求，单路失败不阻塞其他，通过 optional chaining 传默认值。

#### 3.3.2 Executions 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `Execution[]` | `executions` | `GET /api/v1/executions` | `listExecutions({ status, limit, offset })` |
| `ExecutionDetail` | `executionDetail` | `GET /api/v1/executions/{id}` + `GET /api/v1/executions/{id}/graph` + `GET /api/v1/executions/{id}/state` + `GET /api/v1/executions/{id}/analysis/slow-nodes` + `GET /api/v1/executions/{id}/analysis/decision-points` + `GET /api/v1/executions/{id}/analysis/failure-nodes` + `GET /api/v1/executions/{id}/analysis/critical-path` | `getExecutionDetail(id)` → 并行聚合 |
| `Metric[]` | `overviewMetrics` | `GET /api/v1/analysis/stats` | `getOverviewMetrics()` |

#### 3.3.3 Agent Loops 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `AgentLoop[]` | `agentLoops` | `GET /api/v1/agent-loops/summaries` | `listAgentLoops({ status, limit, offset })` |
| `AgentLoopDetail` | `loopDetail` | `GET /api/v1/agent-loops/{id}` + `GET /api/v1/agent-loops/{id}/summary` + `GET /api/v1/agent-loops/{id}/graph` + `GET /api/v1/agent-loops/{id}/variables` + `GET /api/v1/agent-loops/{id}/analysis/root-cause` + `GET /api/v1/agent-loops/{id}/analysis/error-chain` | `getAgentLoopDetail(id)` → 并行聚合 |
| `LoopMessage[]` | `loopMessages` | `GET /api/v1/agent-loops/{id}/messages` | `listLoopMessages(id)` |
| `LoopVariable[]` | `loopVariables` | `GET /api/v1/agent-loops/{id}/variables` | `listLoopVariables(id)` |

#### 3.3.4 Checkpoints 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `Checkpoint[]` | `checkpoints` | `GET /api/v1/checkpoints` + `GET /api/v1/agent-loops/{id}/checkpoints`（按页上下文） | `listCheckpoints({ entityId?, limit?, offset? })` |
| `FileChange[]` | `fileChanges` | `GET /api/v1/files/changes`（若存在）或从 checkpoint entity metadata 拉取 | `listFileChanges({ entityId })` |
| `Approval[]` | `approvals` | `GET /api/v1/approvals/pending` | `listPendingApprovals()` |

#### 3.3.5 Triggers 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `TriggerRecord[]` | `triggerRecords` | `GET /api/v1/trigger-executions` | `listTriggerExecutions({ limit, offset })` |
| `Hook[]` | `hooks` | 后端无对等模型——hooks 是配置项，从 `configs/server/` 层面读取 | **需后端补充** 或改从 `/api/v1/system/config` 类端点下发。**标记为 §6 待确认项**。 |
| `TimelineEntry[]` | `executionTimeline` | `GET /api/v1/events/timeline/{executionId}` | `getExecutionTimeline(executionId)` |

#### 3.3.6 Resources 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `ModelProfile[]` | `modelProfiles` | `GET /api/v1/agents`（agent profiles 就是模型配置）或 `GET /api/v1/llm/profiles`（若 llm profiles 端点存在） | `listModelProfiles()` |
| `Provider[]` | `providers` | `GET /api/v1/providers` | `listProviders()` |
| `Tool[]` | `tools` | `GET /api/v1/tools` | `listTools()` |
| `Script[]` | `scripts` | `GET /api/v1/scripts` | `listScripts()` |
| `Skill[]` | `skills` | `GET /api/v1/skills` | `listSkills()` |
| **toggle enable** | fixtures 本地 | `POST /api/v1/tools/{id}/enable` 或 `PATCH /api/v1/tools/{id}` | `setToolEnabled(id, enabled)` / `setSkillEnabled(id, enabled)` |

#### 3.3.7 Insights 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `QueryResult` | `queryResult` | `POST /api/v1/query` | `runQuery(statement)` |
| `AuditReport[]` | `auditReports` | `GET /api/v1/audit/reports` | `listAuditReports({ limit, offset })` |
| `ErrorAnalysis[]` | `errorAnalyses` | `GET /api/v1/error-analysis` 或 `GET /api/v1/agent-executions/{id}/errors/statistics/advanced` | `listErrorAnalyses()` |
| `PerfNode[]` | `perfNodes` | `GET /api/v1/analysis/performance/compare` 或 `GET /api/v1/workflows/{id}/analysis/performance` | `listPerformanceNodes()` |

#### 3.3.8 Events 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `EventRecord[]` | `events` | `GET /api/v1/events` + `GET /api/v1/events/size` + `GET /api/v1/events/search` | `listEvents({ limit, offset, query? })` |
| `Dependency[]` | `dependencies` | `GET /api/v1/dependencies/audit` + `GET /api/v1/dependencies/impact` | `listDependencies()` |
| `Diagnostic[]` | `diagnostics` | `GET /api/v1/storage/diagnose` + `GET /api/v1/health` | `getDiagnostics()` |

#### 3.3.9 Templates 域

| 前端 ViewModel | fixture 函数 | 后端 API | service 函数 |
|---------------|-------------|---------|-------------|
| `Template[]` | `templates` | `GET /api/v1/templates`（query `kind`） | `listTemplates({ kind?, featured? })` |
| — | — | `GET /api/v1/templates/library/featured` | `listFeaturedTemplates()` |
| — | — | `GET /api/v1/templates/library/popular` | `listPopularTemplates()` |

#### 3.3.10 Settings 域

| 前端 ViewModel | 数据源 | 后端 API | service 函数 |
|---------------|--------|---------|-------------|
| `preferences` (local store) | `stores/preferences.svelte.ts` | `GET /api/v1/preferences` + `PUT /api/v1/preferences` | `loadPreferences()` / `savePreferences(values)` |

### 3.4 SSR / 加载时机

`+layout.ts` 当前 `ssr=false`。接线后的加载策略：

- **纯 Client Load**（保持 ssr=false，改动最小）：
  - 页面 `+page.svelte` 内使用 `onMount` 或 `sveltekit:$browser` 守卫发起请求
  - 优点：SPA 启动最快，无需改造路由结构
  - 缺点：首屏可能先闪空再填数据
- **SvelteKit load()**（推荐长期方案）：
  - 为每个页面新增 `+page.ts`（SvelteKit 4 文件约定），导出 `export const load = async () => { data: await service.xxx() }`
  - 在 `+page.svelte` 中用 `{ data }` 使用
  - 优点：数据就绪后渲染，消除闪空
  - 注意：即使 `ssr=false`，CSR 模式下 load 也会在客户端运行一次

**分阶段**：第一阶段用纯 Client Load 快速接线并验证后端；稳定后再切换到 `+page.ts load()` 模式。

### 3.5 加载态与错误态

Service 层不直接处理 UI 状态——**页面组件负责**。推荐模式：

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { listWorkflows } from '$lib/services/workflows';
  import type { Workflow } from '$lib/types/models';

  let items: Workflow[] | null = $state(null);
  let error: string | null = $state(null);

  onMount(async () => {
    try {
      const res = await listWorkflows({ limit: 50 });
      items = res.items;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Unknown error';
    }
  });
</script>

{#if items === null}<PageSkeleton />
{:else if error}<ErrorState {error} onRetry={() => items = null} />
{:else if items.length === 0}<EmptyState />
{:else}<!-- render list -->{/if}
```

---

## 4. 路由到 service 函数的调用约定

### 4.1 统一规则

1. **列表页**一律传 `limit`（默认 50）和 `offset`（默认 0），支持后续加 `status` / `query` 过滤参数
2. **详情页**一律传 id，service 内部并行聚合多子端点，返回完整 ViewModel
3. **写操作**（toggle enable、batch cancel 等）在 service 层暴露独立函数，调用成功后触发前端局部刷新

### 4.2 写操作清单

| 页面 | 操作 | 后端 API |
|------|------|---------|
| workflows | 创建/更新/删除/版本回滚 | POST/PUT/DELETE `/api/v1/workflows`, `POST /api/v1/workflows/{id}/rollback` |
| agent-loops | 启动/暂停/恢复/取消/状态切换 | POST `/api/v1/agent-loops/{id}/run|pause|resume|cancel`, POST `/api/v1/agent-loops/{id}/status/transition` |
| agent-loops | 保存消息/变量 | POST `/api/v1/agent-loops/{id}/messages`, PUT `/api/v1/agent-loops/{id}/variables/{name}` |
| checkpoints | 创建 checkpoint, 恢复 checkpoint | POST `/api/v1/agent-loops/{id}/checkpoints`, POST `/api/v1/agent-loops/{id}/checkpoints/{cid}/restore` |
| executions | 批量取消/删除 | POST `/api/v1/executions/batch-cancel`, POST `/api/v1/executions/batch-delete` |
| resources | 工具/脚本/技能启停用 | POST `/api/v1/tools/{id}/enable` |
| settings | 偏好保存 | PUT `/api/v1/preferences` |

---

## 5. 页面接线详细计划（按路由）

### 5.1 `routes/workflows/+page.svelte`

**当前 fixture 引用**：`workflows`, `workflowDetail`

**改造要点**：
1. 列表 `workflows` → `listWorkflows({ status })`
2. 详情 `workflowDetail` → `getWorkflowDetail(selectedId)`（由 `selectedId` 决定）
3. 过滤 `status` 移到后端查询参数，不做前端 `.filter()`
4. `filtered.length` 从 `items.length` 直接拿（分页就是过滤后的）

### 5.2 `routes/workflows/[id]/+page.svelte`

**改造要点**：从 `page.params.id` 拿 id → `getWorkflowDetail(id)`

### 5.3 `routes/agent-loops/+page.svelte`

**当前 fixture 引用**：`agentLoops`, `loopDetail`

**改造要点**：同 workflows 模式。`listAgentLoops({ status })` + `getAgentLoopDetail(selectedId)`

### 5.4 `routes/agent-loops/[id]/+page.svelte`

**当前 fixture 引用**：`agentLoops`, `loopDetail`, `loopMessages`, `loopVariables`, `checkpoints`

**改造要点**：
- Messages tab → `listLoopMessages(id)`
- Variables tab → `listLoopVariables(id)`
- Graph tab → 从 `getAgentLoopDetail` 聚合
- Analysis tab → 同上
- Checkpoints tab → `listCheckpoints({ entityId: id })`

### 5.5 `routes/executions/+page.svelte`

**当前 fixture 引用**：`executions`, `executionDetail`, `overviewMetrics`

**改造要点**：
- 列表 → `listExecutions({ status })`
- 详情 → `getExecutionDetail(selectedId)`
- 概览 metric card → `getOverviewMetrics()`（或用 `GET /api/v1/analysis/stats`）

### 5.6 `routes/executions/[id]/+page.svelte`

**改造要点**：从 params.id → `getExecutionDetail(id)`

### 5.7 `routes/checkpoints/+page.svelte`

**当前 fixture 引用**：`checkpoints`, `fileChanges`, `approvals`

**改造要点**：
- Checkpoint chain → `listCheckpoints()`
- Files workspace → `listFileChanges({ entityId })` — **§6 需确认后端路径**
- Approvals → `listPendingApprovals()`

### 5.8 `routes/events/+page.svelte`

**当前 fixture 引用**：`events`, `dependencies`, `diagnostics`

**改造要点**：
- Event stream tab → `listEvents({ limit, query })`；如需实时，改接 `events/stream` SSE
- Dependencies tab → `listDependencies()`（聚合 audit + impact）
- Operations tab → `getDiagnostics()`（聚合 health + storage diagnose）

### 5.9 `routes/insights/+page.svelte`

**当前 fixture 引用**：`auditReports`, `errorAnalyses`, `perfNodes`, `queryResult`

**改造要点**：
- Query tab → `runQuery(statement)`（POST `/api/v1/query`）
- Audit tab → `listAuditReports()`
- Errors tab → `listErrorAnalyses()`
- Performance tab → `listPerformanceNodes()`

### 5.10 `routes/resources/+page.svelte`

**当前 fixture 引用**：`modelProfiles`, `providers`, `tools`, `scripts`, `skills`

**改造要点**：
- Models tab → `listModelProfiles()` + `listProviders()`
- Tools tab → `listTools()` + toggle 走 `setToolEnabled()`
- Scripts tab → `listScripts()`
- Skills tab → `listSkills()` + toggle 走 `setSkillEnabled()`

### 5.11 `routes/triggers/+page.svelte`

**当前 fixture 引用**：`triggerRecords`, `hooks`

**改造要点**：
- Trigger records tab → `listTriggerExecutions({ limit })`
- Hooks tab → hooks 是配置项，**§6 需确认**（fixture 的 `hooks` 实际是静态配置展示，可能不需要后端实时数据，或者用 `/api/v1/hooks` 端点拉取）

### 5.12 `routes/templates/+page.svelte`

**当前 fixture 引用**：`templates`

**改造要点**：
- → `listTemplates({ kind, featuredOnly })`

### 5.13 `routes/settings/+page.svelte`

**当前数据源**：`stores/preferences.svelte.ts`（localStorage）

**改造要点**：
- 页面 mount 时 `loadPreferences()` 初始化 store
- 保存偏好时 `savePreferences(preferences)` — PUT `/api/v1/preferences`
- 后端可能暂无 preferences 持久化，**§6 需确认后端该路由是否已实现**

---

## 6. 阻塞/待确认项

### 6.1 需后端确认的端点

| # | 问题 | 影响页面 | 临时处理 |
|---|------|---------|---------|
| 1 | `/api/v1/files/changes` 或文件工作区变更历史端点是否存在 | checkpoints | 先隐藏 Files tab，待后端确认 |
| 2 | `/api/v1/preferences` 读写端点是否可用（web preferences 模块） | settings | 先保留 localStorage 兜底，后端可用后替换 |
| 3 | hooks 端点 `/api/v1/hooks` 是否存在 | triggers | 先隐藏 Hooks tab 或展示静态配置（`configs/server/`） |
| 4 | `/api/v1/llm/profiles` 与 `/api/v1/agents` 的关系：model profiles 端点命名 | resources | 确认后再定义 DTO |
| 5 | `/api/v1/query`（即席 SQL 查询）是否真实实现 | insights | 若未实现，Query tab 先隐藏或 mock 后端为固定查询 |
| 6 | `/api/v1/error-analysis`（错误分析聚合）端点 | insights errors tab | 若不存在，先从 `agent-executions/{id}/errors/statistics/advanced` 逐回路聚合 |
| 7 | `/api/v1/diagnostics` 综合诊断端点 | events operations tab | 确认 health + storage 端点是否足够 |

### 6.2 需前端确认的行为

| # | 问题 | 建议 |
|---|------|------|
| 1 | `[id]` 详情页如何在 URL 变化时重新加载数据 | 监听 `$page.params.id` 变化 → 触发 `onMount` 或改用 `load()` |
| 2 | SSE 事件流自动重连策略 | 指数退避 + 最后事件 id 续接（后端支持的前提下） |
| 3 | Token 过期/API Key 无效后处理 | 前端 toast + 设置页跳转；后端暂不支持刷新，保持硬失效 |

---

## 7. 执行计划（分批）

### Batch 0：基础设施（所有后续批次的前置）

- [ ] 安装 `openapi-fetch` 依赖
- [ ] 创建 `src/lib/api/client.ts` + `src/lib/api/envelope.ts`
- [ ] vite proxy 配置开发期 `/api` `/health` `/info` `/ws`
- [ ] 联调验证：`call(client.GET('/health'))` 能拿到后端真实数据
- [ ] 鉴权：开发期 `.env` 配置 API Key，生产从后端同源部署省略
- [ ] **删除** `fixtures/` 目录所有导出数据（或重命名为 `fixtures/_removed_`）——避免新旧混用

### Batch 1：只读域（低风险、无写操作、直接替换）

- [ ] `resources/` → `services/resources.ts`
- [ ] `templates/` → `services/templates.ts`
- [ ] `insights/`（audit + errors + performance tabs，query tab 暂放 Batch 2）→ `services/insights.ts`
- [ ] `events/`（dependencies + operations tabs，stream 暂放 Batch 2）→ `services/events.ts`
- 验证命令：每批完成后 `npm run check` 通过 + 手点页面无红线

### Batch 2：核心列表 + 详情（聚合复杂度最高）

- [ ] `workflows/` 列表 + `[id]/` 详情 → `services/workflows.ts`（5 路并行聚合）
- [ ] `agent-loops/` 列表 + `[id]/` 详情 → `services/agent-loops.ts`（7 路并行聚合）
- [ ] `executions/` 列表 + `[id]/` 详情 → `services/executions.ts`（8 路并行聚合）
- [ ] `checkpoints/` → `services/checkpoints.ts`
- [ ] `triggers/` → `services/triggers.ts`
- [ ] `settings/` → `services/settings.ts`（preferences 持久化）

### Batch 3：写操作 + SSE/WS

- [ ] agent loop 启停/暂停/恢复/取消
- [ ] execution 批量取消/删除
- [ ] checkpoint 创建/恢复
- [ ] workflow 版本回滚
- [ ] tool/script/skill 启停用 toggle
- [ ] SSE 事件流（`/api/v1/events/stream`）
- [ ] WebSocket（`/ws`）用于实时更新

### Batch 4：收尾优化

- [ ] 全部页面从 `onMount + async/await` 切换到 SvelteKit `+page.ts load()`
- [ ] 添加全局 loading skeleton
- [ ] 添加全局错误边界
- [ ] 清理已废弃的 DTO 适配层代码
- [ ] 端到端联调测试（`npm run dev` + 启动 wf-server）
- [ ] 确认 `schema.d.ts` 与实际响应一致（**可能需执行 `WF_REFRESH_OPENAPI=1 cargo test -p wf-server committed_snapshot_matches_document` 刷新快照**）

---

## 8. 质量门禁

每批完成后必须通过：

| 检查项 | 命令 |
|--------|------|
| 类型检查 | `npm run check`（svelte-check） |
| ESLint | `npm run lint` |
| 格式化 | `npm run format:check` |
| 单元测试 | `npm run test` |
| **无 fixture 引用** | `grep -r "fixtures" src/routes/ | grep -v "_removed_" |
| **无 hardcoded data** | `grep -r "'trg-\|'wf-\|'exec-\|'agent-\|minutesAgo(" src/routes/ src/lib/services/` |
| 后端快照同步 | `cargo test -p wf-server committed_snapshot_matches_document` |

---

## 9. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| 后端某些端点响应体比 OpenAPI 快照更复杂，unknown 字段嵌套深 | service 层映射繁琐 | 在 DTO 接口里声明必要字段，用 `unknown` 承接其余——页面只消费 ViewModel 层，DTO 多几个字段不影响 |
| OpenAPI 快照未覆盖某些端点（后端直接返回 JSON 但未注册 utoipa） | 前端无法类型化 | 先实现 service 层时手动确认响应形状，提交前刷新快照（Batch 4 的 gate） |
| 详情页多路并行请求中有一路失败 → 页面部分数据缺失 | 用户体验 | 用 `allSettled` + 降级默认值，不在 service 层抛，交给页面决定是否展示占位符 |
| SSE/WS 在 SSR 模式下不可用（虽然当前 ssr=false） | 将来切 SSR 时出问题 | service 层封装里用 `typeof EventSource !== 'undefined'` 守卫 |
| 后端无偏好持久化端点 | settings 页 | 先保留 localStorage 作为 fallback，API 层准备好，后端可用后一行切换 |

---

## 10. 最终交付标准

- [x] `apps/web-app/src/lib/fixtures/` 目录**零页面引用**（仅剩可选 `clock.ts` 辅助）
- [x] `apps/web-app/src/lib/services/` 完整覆盖 10 个路由域
- [x] `apps/web-app/src/lib/api/client.ts` + `envelope.ts` 为唯一请求通道
- [x] **没有任何硬编码的 workflow/execution/agentLoop/checkpoint/trigger/template/tool/script/skill 数据字符串**
- [x] `npm run check` + `npm run lint` + `npm run test` 全部通过
- [x] 启动 wf-server 并访问所有页面能看到真实后端数据
- [x] schema.d.ts 与后端 OpenAPI 快照同步（`cargo test -p wf-server committed_snapshot_matches_document` 通过）

---

## 附录 A：目录结构（接线完成后）

```
apps/web-app/src/
├── lib/
│   ├── api/
│   │   ├── schema.d.ts          ← 保留（自动生成的类型）
│   │   ├── client.ts            ← 新增
│   │   └── envelope.ts          ← 新增
│   ├── services/                ← 新增目录
│   │   ├── index.ts
│   │   ├── workflows.ts
│   │   ├── executions.ts
│   │   ├── agent-loops.ts
│   │   ├── checkpoints.ts
│   │   ├── triggers.ts
│   │   ├── resources.ts
│   │   ├── insights.ts
│   │   ├── events.ts
│   │   ├── templates.ts
│   │   ├── settings.ts
│   │   ├── queries.ts
│   │   ├── sse.ts
│   │   └── ws.ts
│   ├── fixtures/
│   │   └── _removed_/           ← 原文件全部移入
│   ├── stores/                  ← 保留（preferences 需接后端但仍用 store 模式）
│   ├── types/models.ts          ← 保留（ViewModel 层）
│   ├── utils/                   ← 保留
│   └── components/              ← 保留（无修改）
└── routes/                      ← 所有页面去掉 fixture import，改用 service
```

## 附录 B：后端 API 快速参考（按前端消费域分组）

### workflows 域
```
GET  /api/v1/workflows                         ← list (status/limit/offset)
GET  /api/v1/workflows/{id}                    ← detail
GET  /api/v1/workflows/{id}/graph             ← graph nodes+edges
GET  /api/v1/workflows/{id}/versions           ← version history
GET  /api/v1/workflows/{id}/drafts             ← drafts
GET  /api/v1/workflows/{id}/neighbors          ← graph neighbors
GET  /api/v1/workflows/{id}/executions         ← executions of this workflow
GET  /api/v1/workflows/{id}/execution-path     ← execution path
GET  /api/v1/workflows/{id}/analysis/slow-nodes
GET  /api/v1/workflows/{id}/analysis/decision-points
GET  /api/v1/workflows/{id}/analysis/failure-nodes
GET  /api/v1/workflows/{id}/analysis/critical-path
POST /api/v1/workflows                         ← create
PUT  /api/v1/workflows/{id}                    ← update
DELETE /api/v1/workflows/{id}                  ← delete
```

### agent-loops 域
```
GET  /api/v1/agent-loops/summaries             ← list (live + persisted merged)
GET  /api/v1/agent-loops/{id}                  ← detail
GET  /api/v1/agent-loops/{id}/summary          ← execution summary
GET  /api/v1/agent-loops/{id}/messages         ← conversation messages
GET  /api/v1/agent-loops/{id}/variables        ← loop variables
GET  /api/v1/agent-loops/{id}/graph            ← decision graph
GET  /api/v1/agent-loops/{id}/analysis/root-cause
GET  /api/v1/agent-loops/{id}/analysis/error-chain
GET  /api/v1/agent-loops/{id}/analysis/tool-frequency
GET  /api/v1/agent-loops/{id}/iteration-history
POST /api/v1/agent-loops/{id}/run              ← start loop
POST /api/v1/agent-loops/{id}/pause            ← pause running loop
POST /api/v1/agent-loops/{id}/resume           ← resume paused loop
POST /api/v1/agent-loops/{id}/cancel           ← cancel loop
POST /api/v1/agent-loops/{id}/checkpoints      ← create checkpoint
```

### executions 域
```
GET  /api/v1/executions                         ← list
GET  /api/v1/executions/{id}                    ← detail
GET  /api/v1/executions/{id}/graph             ← execution graph
GET  /api/v1/executions/{id}/state              ← execution state snapshot
GET  /api/v1/executions/{id}/timeline           ← execution timeline
GET  /api/v1/executions/{id}/context            ← execution context
GET  /api/v1/executions/{id}/analysis/slow-nodes
GET  /api/v1/executions/{id}/analysis/decision-points
GET  /api/v1/executions/{id}/analysis/failure-nodes
GET  /api/v1/executions/{id}/analysis/critical-path
POST /api/v1/executions/batch-cancel
POST /api/v1/executions/batch-delete
```

### analysis / insights 域
```
GET  /api/v1/analysis/stats
GET  /api/v1/analysis/llm-metrics
GET  /api/v1/analysis/stats/top-workflows
GET  /api/v1/analysis/stats/top-node-types
GET  /api/v1/analysis/stats/agent-profiles
GET  /api/v1/analysis/performance/compare
POST /api/v1/query                              ← ad-hoc SQL query
GET  /api/v1/audit/reports
```

### events 域
```
GET  /api/v1/events                             ← list
GET  /api/v1/events/search                      ← search
GET  /api/v1/events/size
GET  /api/v1/events/stats
GET  /api/v1/events/timeline/{executionId}
GET  /api/v1/events/stream                      ← SSE
GET  /api/v1/dependencies/audit
GET  /api/v1/dependencies/impact
GET  /health
GET  /api/v1/storage/diagnose
```

### resources 域
```
GET  /api/v1/agents                             ← agent profiles (= model configs)
GET  /api/v1/tools                              ← tool registry
GET  /api/v1/scripts                            ← scripts
GET  /api/v1/skills                             ← skills
GET  /api/v1/providers                          ← LLM providers
POST /api/v1/tools/{id}/enable                  ← 或通过 PATCH toggle
POST /api/v1/scripts/{id}/enable
POST /api/v1/skills/{id}/enable
```

### triggers 域
```
GET  /api/v1/trigger-executions                 ← list
GET  /api/v1/trigger-executions/by-workflow/{wf}
GET  /api/v1/trigger-executions/by-trigger/{name}
GET  /api/v1/hooks                              ← hook 列表（若存在）
GET  /api/v1/hooks/{name}/dispatch              ← 测试 hook 触发
```

### checkpoints 域
```
GET  /api/v1/checkpoints                        ← list all
GET  /api/v1/checkpoints/entity/{entityId}      ← by entity
GET  /api/v1/checkpoints/entity/{entityId}/latest
GET  /api/v1/checkpoints/time-range
GET  /api/v1/agent-loops/{id}/checkpoints       ← by loop
GET  /api/v1/agent-loops/{id}/checkpoints/chain
POST /api/v1/agent-loops/{id}/checkpoints       ← create
POST /api/v1/agent-loops/{id}/checkpoints/{cid}/restore
```

### templates 域
```
GET  /api/v1/templates                          ← list (kind query)
GET  /api/v1/templates/library/featured
GET  /api/v1/templates/library/popular
GET  /api/v1/templates/library/summaries
```

### preferences 域
```
GET  /api/v1/preferences
PUT  /api/v1/preferences
GET  /api/v1/preferences/{key}
PUT  /api/v1/preferences/{key}
DELETE /api/v1/preferences/{key}
```

### 系统根路径
```
GET  /health                                    ← 无需鉴权
GET  /info                                      ← 无需鉴权
GET  /                                          ← ApiInfoView
GET  /api-docs/openapi.json                     ← debug build
```

### 实时通道
```
GET  /api/v1/events/stream                      ← SSE（鉴权同 REST）
WS   /ws                                        ← WebSocket（x-api-key header 或 query param）
```
