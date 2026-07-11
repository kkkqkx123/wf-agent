# Web App 前端实现路线图和架构细节

## 概述

本文档提供具体的实现指南、架构图和代码骨架，用于快速启动 Web App 前端的开发。

---

## 目录

1. [系统架构图](#系统架构图)
2. [文件创建清单](#文件创建清单)
3. [代码骨架](#代码骨架)
4. [数据流详解](#数据流详解)
5. [集成检查点](#集成检查点)

---

## 系统架构图

### 高层架构

```
┌─────────────────────────────────────────────────────────────┐
│                    SvelteKit Web App                         │
│  ┌──────────────────────────────────────────────────────┐   │
│  │           Pages & Components Layer                   │   │
│  │  (routes/, routes/workflows/, routes/executions/)    │   │
│  └───────────────────┬──────────────────────────────────┘   │
│                      │                                       │
│  ┌───────────────────▼──────────────────────────────────┐   │
│  │           Svelte Stores Layer                        │   │
│  │  (lib/stores/workflows.ts, executions.ts, ui.ts)     │   │
│  └───────────────────┬──────────────────────────────────┘   │
│                      │                                       │
│  ┌───────────────────▼──────────────────────────────────┐   │
│  │              API Client Layer                        │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────┐ │   │
│  │  │ HTTP Client  │  │ Workflows    │  │ Executions │ │   │
│  │  └──────────────┘  │ Executions   │  │ Events/SSE │ │   │
│  │                    │ Events       │  └────────────┘ │   │
│  │                    └──────────────┘                  │   │
│  └───────────────────┬──────────────────────────────────┘   │
└────────────────────┬─┴─────────────────────────────────────┘
                     │
        ┌────────────┴────────────┐
        │                         │
   HTTP/REST              Server-Sent Events
   (Request-Response)     (Real-time Stream)
        │                         │
        ▼                         ▼
┌────────────────────────────────────────────┐
│         Express API Server                 │
│  ┌──────────────────────────────────────┐  │
│  │  REST Routes Layer                   │  │
│  │  /api/v1/workflows/*                 │  │
│  │  /api/v1/executions/*                │  │
│  │  /api/v1/events/executions/:id       │  │
│  └──────────────┬───────────────────────┘  │
│                 │                          │
│  ┌──────────────▼───────────────────────┐  │
│  │  Adapters & Services Layer           │  │
│  │  WorkflowAdapter                      │  │
│  │  ExecutionService                     │  │
│  │  EventManager                         │  │
│  └──────────────┬───────────────────────┘  │
│                 │                          │
│  ┌──────────────▼───────────────────────┐  │
│  │  SDK & Storage Layer                 │  │
│  │  SDK Instance                         │  │
│  │  StorageManager                       │  │
│  └──────────────────────────────────────┘  │
└────────────────────────────────────────────┘
```

### 数据流序列图

#### 创建执行 + 实时监控

```
前端                   后端
  │                     │
  ├─ POST /executions ──>│
  │                     │ 创建执行
  │                     │ 发出 "status" 事件
  │<─ 201 executionId ──┤
  │                     │
  ├─ GET /events/... ───> EventSource 连接
  │<────────────────────┤ SSE 握手完成
  │                     │
  │                     │ 执行进行中...
  │                     │ 发出 "progress" 事件
  │<─── data: {...} ────┤
  │  处理事件           │
  │                     │ 发出 "log" 事件
  │<─── data: {...} ────┤
  │  追加日志           │
  │                     │ ...
  │                     │
  │                     │ 执行完成
  │                     │ 发出 "complete" 事件
  │<─── data: {...} ────┤
  │  关闭连接           │ EventSource 关闭
  ├ close EventSource ──>│
  │                     │
```

### 组件通信图

```
+page.svelte (Executions Detail)
    │
    ├─ 调用 createExecution()
    │   └─ executions.ts (Store)
    │       ├─ 调用 api.create()
    │       │   └─ api/executions.ts
    │       │       └─ apiClient.request()
    │       │           └─ api/client.ts → HTTP POST
    │       ├─ 订阅 subscribeToExecution()
    │       │   └─ api/events.ts
    │       │       └─ new EventSource() → SSE
    │       └─ 更新 executions store
    │
    └─ 显示 <ExecutionMonitor />
        └─ 订阅 selectedExecution store
            └─ 实时显示状态、进度、日志
```

---

## 文件创建清单

### API 层 (5 个文件)

```
lib/api/
├── client.ts                 # HTTP 基础客户端
├── workflows.ts              # 工作流 API 模块
├── executions.ts             # 执行 API 模块
├── events.ts                 # SSE 事件管理
└── index.ts                  # 导出
```

### Store 层 (4 个文件)

```
lib/stores/
├── workflows.ts              # 工作流存储
├── executions.ts             # 执行存储
├── ui.ts                      # UI 状态存储
└── index.ts                   # 导出
```

### Types 层 (3 个文件)

```
lib/types/
├── workflow.ts               # 工作流类型
├── execution.ts              # 执行类型
└── api.ts                     # API 通用类型
```

### Components 层 (8+ 个文件)

```
lib/components/
├── layout/
│   ├── Header.svelte
│   ├── Sidebar.svelte
│   └── Footer.svelte
├── workflows/
│   ├── WorkflowList.svelte
│   ├── WorkflowCard.svelte
│   ├── WorkflowGraph.svelte
│   └── WorkflowForm.svelte
├── executions/
│   ├── ExecutionMonitor.svelte
│   ├── ExecutionLog.svelte
│   ├── ExecutionStats.svelte
│   └── ExecutionActions.svelte
└── common/
    ├── Loading.svelte
    ├── Error.svelte
    ├── Modal.svelte
    └── Button.svelte
```

### Routes 层 (6+ 个文件)

```
routes/
├── +layout.svelte            # Root layout
├── +page.svelte              # Home
├── workflows/
│   ├── +page.svelte          # 工作流列表
│   ├── +page.server.ts       # 服务端逻辑
│   └── [id]/
│       ├── +page.svelte      # 工作流详情
│       └── +page.server.ts
└── executions/
    ├── +page.svelte          # 执行列表
    ├── +page.server.ts
    └── [id]/
        ├── +page.svelte      # 执行详情
        ├── +page.server.ts
        └── monitor/
            └── +page.svelte
```

### 配置文件 (3 个文件)

```
根目录
├── .env.local                # 本地配置（开发）
├── .env.production           # 生产配置
└── vite.config.ts            # Vite 配置

前端根目录
├── svelte.config.js          # SvelteKit 配置
├── tsconfig.json             # TypeScript 配置
└── package.json              # 已更新
```

---

## 代码骨架

### 1. API Client 基础

**文件**: `lib/api/client.ts`

```typescript
import { env } from '$env/dynamic/public';

export interface ApiResponse<T = any> {
  success: boolean;
  data?: T;
  error?: {
    code: string;
    message: string;
    details?: Record<string, any>;
  };
  meta?: {
    timestamp: string;
    path: string;
    method: string;
  };
}

export class ApiError extends Error {
  constructor(
    message: string,
    public code: string,
    public status: number,
    public details?: Record<string, any>
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = 'http://localhost:3000') {
    this.baseUrl = baseUrl;
  }

  async request<T = any>(
    method: string,
    path: string,
    options?: {
      query?: Record<string, any>;
      body?: Record<string, any>;
      headers?: Record<string, string>;
    }
  ): Promise<T> {
    const url = new URL(`${this.baseUrl}/api/v1${path}`);

    if (options?.query) {
      Object.entries(options.query).forEach(([key, value]) => {
        if (value !== undefined && value !== null) {
          url.searchParams.append(key, String(value));
        }
      });
    }

    const response = await fetch(url.toString(), {
      method,
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
      body: options?.body ? JSON.stringify(options.body) : undefined,
    });

    const data: ApiResponse<T> = await response.json();

    if (!data.success) {
      throw new ApiError(
        data.error?.message || 'Unknown error',
        data.error?.code || 'UNKNOWN_ERROR',
        response.status,
        data.error?.details
      );
    }

    return data.data as T;
  }

  getBaseUrl(): string {
    return this.baseUrl;
  }
}

// 单例
export const apiClient = new ApiClient(
  env.PUBLIC_API_BASE_URL || 'http://localhost:3000'
);
```

### 2. Workflow API

**文件**: `lib/api/workflows.ts`

```typescript
import { apiClient } from './client';
import type { Workflow, CreateWorkflowInput } from '$lib/types/workflow';

export const workflows = {
  async list(params?: { offset?: number; limit?: number }) {
    return apiClient.request<{ data: Workflow[]; meta: { total: number; offset: number; limit: number } }>(
      'GET',
      '/workflows',
      { query: params }
    );
  },

  async get(id: string) {
    return apiClient.request<Workflow>('GET', `/workflows/${id}`);
  },

  async create(input: CreateWorkflowInput) {
    return apiClient.request<Workflow>('POST', '/workflows', { body: input });
  },

  async update(id: string, input: Partial<CreateWorkflowInput>) {
    return apiClient.request<Workflow>('PUT', `/workflows/${id}`, { body: input });
  },

  async delete(id: string) {
    return apiClient.request<void>('DELETE', `/workflows/${id}`);
  },

  async getGraph(id: string) {
    return apiClient.request<any>('GET', `/workflows/${id}/graph`);
  },
};
```

### 3. Workflow Store

**文件**: `lib/stores/workflows.ts`

```typescript
import { writable, derived } from 'svelte/store';
import type { Workflow } from '$lib/types/workflow';
import { workflows as workflowsApi } from '$lib/api/workflows';

export const workflows = writable<Workflow[]>([]);
export const selectedId = writable<string | null>(null);
export const loading = writable(false);
export const error = writable<string | null>(null);

export const selected = derived([workflows, selectedId], ([$workflows, $id]) =>
  $id ? $workflows.find((w) => w.id === $id) : null
);

export async function loadWorkflows(offset = 0, limit = 20) {
  loading.set(true);
  error.set(null);

  try {
    const result = await workflowsApi.list({ offset, limit });
    workflows.set(result.data);
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
    throw err;
  } finally {
    loading.set(false);
  }
}

export async function createWorkflow(input: any) {
  loading.set(true);
  error.set(null);

  try {
    const result = await workflowsApi.create(input);
    workflows.update((list) => [...list, result]);
    return result;
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
    throw err;
  } finally {
    loading.set(false);
  }
}

export async function deleteWorkflow(id: string) {
  loading.set(true);
  error.set(null);

  try {
    await workflowsApi.delete(id);
    workflows.update((list) => list.filter((w) => w.id !== id));
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
    throw err;
  } finally {
    loading.set(false);
  }
}
```

### 4. SSE Events

**文件**: `lib/api/events.ts`

```typescript
import { executions } from '$lib/stores/executions';
import type { Execution, LogEntry } from '$lib/types/execution';

export interface ExecutionEvent {
  type: 'status' | 'log' | 'progress' | 'error' | 'complete';
  executionId: string;
  timestamp: string;
  data: Record<string, any>;
}

const subscriptions = new Map<string, EventSource>();
const logs = new Map<string, LogEntry[]>();

export function subscribe(executionId: string) {
  if (subscriptions.has(executionId)) return;

  const baseUrl = import.meta.env.VITE_API_BASE_URL || 'http://localhost:3000';
  const es = new EventSource(`${baseUrl}/api/v1/events/executions/${executionId}`);

  es.addEventListener('message', (e: MessageEvent) => {
    const event: ExecutionEvent = JSON.parse(e.data);
    handleEvent(event);
  });

  es.addEventListener('error', () => {
    if (es.readyState === EventSource.CLOSED) {
      subscriptions.delete(executionId);
    }
  });

  subscriptions.set(executionId, es);
}

export function unsubscribe(executionId: string) {
  subscriptions.get(executionId)?.close();
  subscriptions.delete(executionId);
  logs.delete(executionId);
}

function handleEvent(event: ExecutionEvent) {
  const { type, executionId, data } = event;

  switch (type) {
    case 'status':
      executions.update((map) => {
        const exec = map.get(executionId);
        if (exec) exec.status = data.status;
        return map;
      });
      break;

    case 'log':
      const entry: LogEntry = {
        timestamp: event.timestamp,
        level: data.level,
        message: data.message,
      };
      const logList = logs.get(executionId) || [];
      logList.push(entry);
      logs.set(executionId, logList);
      break;

    case 'complete':
      unsubscribe(executionId);
      break;
  }
}

export function getLogs(executionId: string): LogEntry[] {
  return logs.get(executionId) || [];
}
```

### 5. 页面示例

**文件**: `routes/workflows/+page.svelte`

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { workflows, loading, error, loadWorkflows } from '$lib/stores/workflows';

  onMount(async () => {
    await loadWorkflows();
  });
</script>

<div class="page">
  <h1>工作流</h1>

  {#if $loading}
    <div class="loading">加载中...</div>
  {:else if $error}
    <div class="error">错误: {$error}</div>
  {:else}
    <div class="workflow-grid">
      {#each $workflows as workflow (workflow.id)}
        <div class="workflow-card">
          <h3>{workflow.name}</h3>
          <p>{workflow.description}</p>
          <div class="meta">
            <span>节点: {workflow.nodeCount}</span>
            <span>边: {workflow.edgeCount}</span>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page {
    max-width: 1200px;
    margin: 0 auto;
    padding: 2rem;
  }

  .workflow-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 1.5rem;
    margin-top: 1rem;
  }

  .workflow-card {
    border: 1px solid #e0e0e0;
    border-radius: 8px;
    padding: 1.5rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .workflow-card:hover {
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
    transform: translateY(-2px);
  }

  .meta {
    display: flex;
    gap: 1rem;
    margin-top: 1rem;
    font-size: 0.85rem;
    color: #666;
  }
</style>
```

---

## 数据流详解

### 场景 1: 加载工作流列表

```
1. 用户访问 /workflows
2. Page 组件挂载 → onMount
3. 调用 loadWorkflows()
   └─ workflows store action
4. loading.set(true)
5. 调用 workflowsApi.list()
   └─ API 客户端
6. fetch GET /api/v1/workflows?offset=0&limit=20
7. 后端返回 { success: true, data: [...], meta: {...} }
8. workflows.set(result.data)
9. loading.set(false)
10. 组件重新渲染，显示列表
```

### 场景 2: 执行工作流 + 实时监控

```
1. 用户点击"执行"按钮
2. 调用 createExecution(workflowId)
   └─ executions store action
3. 调用 executionsApi.create()
   └─ fetch POST /api/v1/executions
4. 后端创建执行，发出 "status" 事件
5. 返回 { executionId, status: "running" }
6. executions store 记录执行
7. 调用 subscribe(executionId)
   └─ new EventSource('/api/v1/events/...')
8. 页面显示 ExecutionMonitor 组件
9. SSE 事件到达 → 处理事件
   - "status" → 更新 execution.status
   - "log" → 追加日志条目
   - "progress" → 更新进度条
   - "complete" → 关闭 EventSource
10. 用户看到实时更新
```

---

## 集成检查点

### Phase 4.1 检查点

- [ ] HTTP 客户端能成功调用后端 /health
- [ ] API 类型定义完整
- [ ] 环境配置正确加载
- [ ] 错误处理能正确捕获 API 错误

### Phase 4.2 检查点

- [ ] Workflow store 能正确存储和检索数据
- [ ] Execution store 支持多执行追踪
- [ ] Store 更新触发组件重新渲染
- [ ] Derived stores 工作正常

### Phase 4.3 检查点

- [ ] 工作流列表页面显示数据
- [ ] 点击卡片能导航到详情页
- [ ] 创建表单能正常提交
- [ ] 删除操作成功更新列表

### Phase 4.4 检查点

- [ ] EventSource 连接成功建立
- [ ] SSE 事件正确接收和解析
- [ ] ExecutionMonitor 实时显示更新
- [ ] 日志自动滚动到底部
- [ ] 连接断开后能自动重连

### Phase 4.5 检查点

- [ ] 单元测试覆盖主要 store 逻辑
- [ ] E2E 测试覆盖主要用户流程
- [ ] 性能指标达标（首屏 < 3s）
- [ ] 错误处理完善（网络错误、API 错误等）

---

## 快速开始命令

```bash
# 1. 安装依赖
cd apps/web-app
pnpm install

# 2. 创建基础文件结构
mkdir -p src/lib/{api,stores,types,components/{layout,workflows,executions,common}}

# 3. 复制代码骨架到相应文件

# 4. 启动后端（另一个终端）
pnpm dev:server

# 5. 启动前端（当前终端）
pnpm dev

# 6. 访问 http://localhost:5173

# 7. 开发过程中运行类型检查
pnpm typecheck

# 8. 构建生产版本
pnpm build
```

---

## 常见问题排查

### API 连接失败

```typescript
// 检查 .env.local
// PUBLIC_API_BASE_URL=http://localhost:3000

// 检查后端是否运行
// http://localhost:3000/health 返回 { status: "ok" }
```

### SSE 连接失败

```javascript
// 检查浏览器控制台
// Network 选项卡 → 查找 EventSource 请求
// 确认响应头包含 Content-Type: text/event-stream
```

### Store 更新不触发重新渲染

```typescript
// 错误: store.update(obj => (obj.prop = value, obj))
// 正确: store.update(obj => ({ ...obj, prop: value }))
```

### TypeScript 类型错误

```bash
# 重新生成 SvelteKit 类型
pnpm svelte-kit sync
```

---

**最后更新**: 2026-07-11
**状态**: 实现路线图完成
