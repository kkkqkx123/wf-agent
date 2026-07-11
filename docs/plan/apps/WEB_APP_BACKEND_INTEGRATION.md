# Web App 前端后端集成分析

## 文档概述

本文档分析 **@wf-agent/web-app** (SvelteKit 前端) 与 **@wf-agent/server** (Express 后端) 的集成方案，包括架构设计、API 调用方式、状态管理和实时通信集成。

---

## 目录

1. [项目概览](#项目概览)
2. [后端 API 规范](#后端-api-规范)
3. [前端架构设计](#前端架构设计)
4. [API 客户端实现](#api-客户端实现)
5. [状态管理](#状态管理)
6. [实时通信集成](#实时通信集成)
7. [组件层级](#组件层级)
8. [开发工作流](#开发工作流)
9. [部署](#部署)

---

## 项目概览

### 前端技术栈

```
SvelteKit 2.59.0
├─ Svelte 5.55.5 (UI 框架)
├─ Vite 8.0.10 (构建工具)
├─ TypeScript 6.0.3 (类型系统)
└─ Adapter: Auto (部署适配器)
```

### 后端技术栈

```
Express 5.2.1
├─ REST API (12 个端点)
├─ SSE Events (实时推送)
├─ SDK Integration (工作流执行)
└─ Dependency Injection (依赖管理)
```

### 通信模型

```
浏览器 (SvelteKit App)
    ↓
HTTP/REST (请求-响应)
    ↓
Express Server
    ├─ /api/v1/workflows/* (CRUD)
    ├─ /api/v1/executions/* (执行管理)
    └─ /api/v1/events/* (SSE 实时流)
    ↓
SDK + Storage
```

---

## 后端 API 规范

### 1. 工作流管理 API

#### 基础端点
```
GET    /api/v1/workflows              # 列表（分页）
GET    /api/v1/workflows/:id          # 单个工作流
POST   /api/v1/workflows              # 创建
PUT    /api/v1/workflows/:id          # 更新
DELETE /api/v1/workflows/:id          # 删除
GET    /api/v1/workflows/:id/graph    # 获取图结构
```

#### 请求/响应格式

```typescript
// POST /api/v1/workflows
Request Body:
{
  "name": string;
  "description"?: string;
  "config"?: Record<string, any>;
}

Response (201):
{
  "success": true,
  "data": {
    "id": string;
    "name": string;
    "description"?: string;
    "config"?: Record<string, any>;
    "createdAt": ISO8601;
    // ...
  },
  "meta": {
    "timestamp": ISO8601;
    "path": string;
    "method": string;
  }
}
```

#### 列表分页
```
GET /api/v1/workflows?offset=0&limit=20

Response:
{
  "success": true,
  "data": [...],
  "meta": {
    "total": number,
    "offset": number,
    "limit": number
  }
}
```

### 2. 执行管理 API

#### 基础端点
```
POST   /api/v1/executions              # 创建执行
GET    /api/v1/executions/:id          # 获取状态
POST   /api/v1/executions/:id/pause    # 暂停
POST   /api/v1/executions/:id/resume   # 恢复
POST   /api/v1/executions/:id/stop     # 停止
GET    /api/v1/executions/:id/logs     # 获取日志
```

#### 创建执行
```typescript
POST /api/v1/executions
Body: {
  "workflowId": string;
  "input"?: Record<string, any>;
}

Response (201):
{
  "success": true,
  "data": {
    "executionId": string;
    "workflowId": string;
    "status": "running" | "paused" | "completed" | "failed" | "cancelled";
    "startTime": ISO8601;
  }
}
```

#### 获取状态
```typescript
GET /api/v1/executions/:id

Response (200):
{
  "success": true,
  "data": {
    "id": string;
    "workflowId": string;
    "status": ExecutionStatus;
    "progress": number;
    "currentNode": string;
    "startTime": ISO8601;
    "endTime"?: ISO8601;
    "error"?: string;
  }
}
```

### 3. 实时事件 API (SSE)

#### 连接端点
```
GET /api/v1/events/executions/:id
```

#### 事件格式

```typescript
interface ExecutionEvent {
  type: "status" | "log" | "progress" | "error" | "complete";
  executionId: string;
  timestamp: ISO8601;
  data: {
    // type-specific fields
  };
}
```

#### 事件类型详解

**Status 事件** - 状态变化
```typescript
{
  "type": "status",
  "data": {
    "status": "running" | "paused" | "completed" | "failed" | "cancelled",
    "message": string
  }
}
```

**Log 事件** - 日志条目
```typescript
{
  "type": "log",
  "data": {
    "level": "debug" | "info" | "warn" | "error",
    "message": string,
    "context"?: string
  }
}
```

**Progress 事件** - 进度更新
```typescript
{
  "type": "progress",
  "data": {
    "current": number,
    "total": number,
    "percentage": number,
    "currentNode": string
  }
}
```

**Error 事件** - 错误通知
```typescript
{
  "type": "error",
  "data": {
    "code": string,
    "message": string,
    "details"?: Record<string, any>
  }
}
```

**Complete 事件** - 执行完成
```typescript
{
  "type": "complete",
  "data": {
    "status": "completed" | "failed" | "cancelled",
    "duration": number, // milliseconds
    "result"?: Record<string, any>
  }
}
```

### 4. 错误处理规范

#### 统一错误格式
```typescript
{
  "success": false,
  "error": {
    "code": string;        // 错误代码
    "message": string;     // 用户消息
    "details"?: object;    // 额外信息
  },
  "meta": {
    "timestamp": ISO8601;
    "path": string;
    "method": string;
  }
}
```

#### HTTP 状态码映射
| 状态码 | 错误代码 | 说明 |
|--------|---------|------|
| 400 | VALIDATION_ERROR | 参数验证失败 |
| 404 | NOT_FOUND | 资源不存在 |
| 409 | CONFLICT_ERROR | 冲突错误 |
| 500 | INTERNAL_ERROR | 服务器错误 |

---

## 前端架构设计

### 1. 项目结构

```
src/
├── routes/
│   ├── +layout.svelte           # Root layout
│   ├── +page.svelte             # Home page
│   ├── workflows/
│   │   ├── +page.svelte         # 工作流列表
│   │   ├── +page.server.ts      # 服务器端逻辑
│   │   └── [id]/
│   │       ├── +page.svelte     # 工作流详情
│   │       ├── +page.server.ts
│   │       └── execute/
│   │           └── +page.svelte # 执行页面
│   └── executions/
│       ├── +page.svelte         # 执行列表
│       └── [id]/
│           ├── +page.svelte     # 执行详情
│           ├── +layout.server.ts
│           └── monitor/
│               └── +page.svelte # 实时监控
├── lib/
│   ├── api/
│   │   ├── client.ts            # HTTP 客户端
│   │   ├── workflows.ts         # 工作流 API
│   │   ├── executions.ts        # 执行 API
│   │   └── events.ts            # SSE 事件
│   ├── stores/
│   │   ├── workflows.ts         # 工作流存储
│   │   ├── executions.ts        # 执行存储
│   │   └── ui.ts                # UI 状态
│   ├── components/
│   │   ├── WorkflowList.svelte
│   │   ├── WorkflowCard.svelte
│   │   ├── ExecutionMonitor.svelte
│   │   ├── ExecutionLog.svelte
│   │   └── ...
│   ├── types/
│   │   ├── workflow.ts
│   │   ├── execution.ts
│   │   └── api.ts
│   └── utils/
│       ├── formatting.ts
│       └── validation.ts
└── app.html
```

### 2. 数据流架构

```
页面组件
    ↓
Svelte Store (状态管理)
    ↓
API 客户端 (HTTP + SSE)
    ↓
后端服务
    ↓
SDK + Storage
```

### 3. 分层设计

#### 层级结构
```
Presentation Layer (Pages + Components)
    ↓ uses
Svelte Stores (State Management)
    ↓ calls
API Layer (HTTP Client)
    ↓ communicates
Backend API (Express)
    ↓ uses
SDK & Storage
```

#### 代码示例

**页面** (routes/workflows/+page.svelte)
```svelte
<script lang="ts">
  import { workflows, loading, error } from '$lib/stores/workflows';
  import { loadWorkflows } from '$lib/api/workflows';

  onMount(async () => {
    await loadWorkflows();
  });
</script>

<div>
  {#if $loading}
    <Loading />
  {:else if $error}
    <Error message={$error} />
  {:else}
    {#each $workflows as workflow}
      <WorkflowCard {workflow} />
    {/each}
  {/if}
</div>
```

**Store** (lib/stores/workflows.ts)
```typescript
import { writable, derived } from 'svelte/store';
import type { Workflow } from '$lib/types/workflow';

export const workflows = writable<Workflow[]>([]);
export const loading = writable(false);
export const error = writable<string | null>(null);

export async function loadWorkflows(offset = 0, limit = 20) {
  loading.set(true);
  error.set(null);
  
  try {
    const result = await apiClient.workflows.list({ offset, limit });
    workflows.set(result.data);
  } catch (err) {
    error.set(err.message);
  } finally {
    loading.set(false);
  }
}
```

**API 客户端** (lib/api/workflows.ts)
```typescript
import { apiClient } from './client';
import type { Workflow, CreateWorkflowInput } from '$lib/types/workflow';

export const workflows = {
  async list(params?: { offset?: number; limit?: number }) {
    return apiClient.request<{ data: Workflow[] }>('GET', '/workflows', {
      query: params,
    });
  },

  async get(id: string) {
    return apiClient.request<{ data: Workflow }>('GET', `/workflows/${id}`);
  },

  async create(input: CreateWorkflowInput) {
    return apiClient.request<{ data: Workflow }>('POST', '/workflows', {
      body: input,
    });
  },

  async update(id: string, input: Partial<CreateWorkflowInput>) {
    return apiClient.request<{ data: Workflow }>('PUT', `/workflows/${id}`, {
      body: input,
    });
  },

  async delete(id: string) {
    return apiClient.request<void>('DELETE', `/workflows/${id}`);
  },

  async getGraph(id: string) {
    return apiClient.request<{ data: WorkflowGraph }>('GET', `/workflows/${id}/graph`);
  },
};
```

---

## API 客户端实现

### 1. HTTP 客户端基础

**文件**: `lib/api/client.ts`

```typescript
import { env } from '$env/dynamic/public';

interface RequestOptions {
  query?: Record<string, any>;
  body?: Record<string, any>;
  headers?: Record<string, string>;
}

interface ApiResponse<T> {
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

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  async request<T>(
    method: string,
    path: string,
    options?: RequestOptions
  ): Promise<T> {
    const url = new URL(`${this.baseUrl}/api/v1${path}`);

    // Add query parameters
    if (options?.query) {
      Object.entries(options.query).forEach(([key, value]) => {
        if (value !== undefined && value !== null) {
          url.searchParams.append(key, String(value));
        }
      });
    }

    const config: RequestInit = {
      method,
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
    };

    if (options?.body) {
      config.body = JSON.stringify(options.body);
    }

    try {
      const response = await fetch(url.toString(), config);
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
    } catch (error) {
      if (error instanceof ApiError) throw error;
      throw new ApiError(
        error instanceof Error ? error.message : 'Network error',
        'NETWORK_ERROR',
        0
      );
    }
  }
}

class ApiError extends Error {
  constructor(
    public message: string,
    public code: string,
    public status: number,
    public details?: Record<string, any>
  ) {
    super(message);
  }
}

export const apiClient = new ApiClient(
  env.PUBLIC_API_BASE_URL || 'http://localhost:3000'
);
```

### 2. 环境配置

**文件**: `src/app.html`

```html
<script>
  // Development: http://localhost:3000
  // Production: https://api.example.com
  window.__API_BASE_URL__ = import.meta.env.VITE_API_BASE_URL || 'http://localhost:3000';
</script>
```

**文件**: `.env.development`

```
PUBLIC_API_BASE_URL=http://localhost:3000
```

**文件**: `.env.production`

```
PUBLIC_API_BASE_URL=https://api.example.com
```

### 3. 类型定义

**文件**: `lib/types/workflow.ts`

```typescript
export interface Workflow {
  id: string;
  name: string;
  description?: string;
  config?: Record<string, any>;
  createdAt: string;
  updatedAt: string;
  nodeCount: number;
  edgeCount: number;
}

export interface CreateWorkflowInput {
  name: string;
  description?: string;
  config?: Record<string, any>;
}

export interface WorkflowGraph {
  id: string;
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
}

export interface WorkflowNode {
  id: string;
  type: string;
  data: Record<string, any>;
  position: { x: number; y: number };
}

export interface WorkflowEdge {
  id: string;
  source: string;
  target: string;
  data?: Record<string, any>;
}
```

**文件**: `lib/types/execution.ts`

```typescript
export type ExecutionStatus =
  | 'pending'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface Execution {
  id: string;
  workflowId: string;
  status: ExecutionStatus;
  progress?: number;
  currentNode?: string;
  startTime: string;
  endTime?: string;
  error?: string;
  input?: Record<string, any>;
  output?: Record<string, any>;
}

export interface LogEntry {
  timestamp: string;
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  context?: string;
}
```

---

## 状态管理

### 1. Svelte Stores 策略

使用 Svelte 内置 Stores 进行状态管理（轻量级、零依赖）：

**文件**: `lib/stores/workflows.ts`

```typescript
import { writable, derived, readable } from 'svelte/store';
import type { Workflow } from '$lib/types/workflow';
import * as api from '$lib/api/workflows';

// Primary stores
export const workflows = writable<Workflow[]>([]);
export const selectedWorkflowId = writable<string | null>(null);
export const loading = writable(false);
export const error = writable<string | null>(null);
export const pagination = writable({ offset: 0, limit: 20, total: 0 });

// Derived store
export const selectedWorkflow = derived(
  [workflows, selectedWorkflowId],
  ([$workflows, $id]) => $workflows.find((w) => w.id === $id)
);

// Actions
export async function loadWorkflows(offset = 0, limit = 20) {
  loading.set(true);
  error.set(null);

  try {
    const result = await api.list({ offset, limit });
    workflows.set(result.data);
    pagination.set({
      offset,
      limit,
      total: result.meta?.total || 0,
    });
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
  } finally {
    loading.set(false);
  }
}

export async function createWorkflow(input: CreateWorkflowInput) {
  loading.set(true);
  error.set(null);

  try {
    const result = await api.create(input);
    workflows.update((list) => [...list, result.data]);
    return result.data;
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
    await api.delete(id);
    workflows.update((list) => list.filter((w) => w.id !== id));
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
    throw err;
  } finally {
    loading.set(false);
  }
}
```

**文件**: `lib/stores/executions.ts`

```typescript
import { writable, derived } from 'svelte/store';
import type { Execution, ExecutionStatus } from '$lib/types/execution';
import * as api from '$lib/api/executions';
import { subscribeToExecution } from '$lib/api/events';

export const executions = writable<Map<string, Execution>>(new Map());
export const selectedExecutionId = writable<string | null>(null);
export const loading = writable(false);
export const error = writable<string | null>(null);

// Derived store
export const selectedExecution = derived(
  [executions, selectedExecutionId],
  ([$executions, $id]) => $id ? $executions.get($id) : null
);

// Actions
export async function createExecution(
  workflowId: string,
  input?: Record<string, any>
) {
  loading.set(true);
  error.set(null);

  try {
    const result = await api.create(workflowId, input);
    const execution: Execution = {
      id: result.data.executionId,
      workflowId,
      status: 'running',
      startTime: new Date().toISOString(),
      input,
    };

    executions.update((map) => new Map(map).set(execution.id, execution));
    selectedExecutionId.set(execution.id);

    // Subscribe to real-time updates
    subscribeToExecution(execution.id);

    return execution;
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
    throw err;
  } finally {
    loading.set(false);
  }
}

export async function pauseExecution(id: string) {
  try {
    await api.pause(id);
    executions.update((map) => {
      const exec = map.get(id);
      if (exec) exec.status = 'paused';
      return map;
    });
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
    throw err;
  }
}

export async function resumeExecution(id: string) {
  try {
    await api.resume(id);
    executions.update((map) => {
      const exec = map.get(id);
      if (exec) exec.status = 'running';
      return map;
    });
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
    throw err;
  }
}

export async function stopExecution(id: string) {
  try {
    await api.stop(id);
    executions.update((map) => {
      const exec = map.get(id);
      if (exec) {
        exec.status = 'cancelled';
        exec.endTime = new Date().toISOString();
      }
      return map;
    });
  } catch (err) {
    error.set(err instanceof Error ? err.message : 'Unknown error');
    throw err;
  }
}
```

---

## 实时通信集成

### 1. SSE 事件客户端

**文件**: `lib/api/events.ts`

```typescript
import { executions } from '$lib/stores/executions';
import type { Execution, LogEntry } from '$lib/types/execution';

interface ExecutionEvent {
  type: 'status' | 'log' | 'progress' | 'error' | 'complete';
  executionId: string;
  timestamp: string;
  data: Record<string, any>;
}

const activeSubscriptions = new Map<string, EventSource>();
const logStore = new Map<string, LogEntry[]>();

export function subscribeToExecution(executionId: string) {
  // Avoid duplicate subscriptions
  if (activeSubscriptions.has(executionId)) {
    return;
  }

  const baseUrl = import.meta.env.VITE_API_BASE_URL || 'http://localhost:3000';
  const eventSource = new EventSource(
    `${baseUrl}/api/v1/events/executions/${executionId}`
  );

  eventSource.addEventListener('message', (e: MessageEvent) => {
    const event: ExecutionEvent = JSON.parse(e.data);
    handleEvent(event);
  });

  eventSource.addEventListener('error', () => {
    if (eventSource.readyState === EventSource.CLOSED) {
      activeSubscriptions.delete(executionId);
    }
  });

  activeSubscriptions.set(executionId, eventSource);
}

export function unsubscribeFromExecution(executionId: string) {
  const eventSource = activeSubscriptions.get(executionId);
  if (eventSource) {
    eventSource.close();
    activeSubscriptions.delete(executionId);
  }
  logStore.delete(executionId);
}

function handleEvent(event: ExecutionEvent) {
  const { type, executionId, data } = event;

  switch (type) {
    case 'status':
      executions.update((map) => {
        const exec = map.get(executionId);
        if (exec) {
          exec.status = data.status;
        }
        return map;
      });
      break;

    case 'log':
      const logEntry: LogEntry = {
        timestamp: event.timestamp,
        level: data.level,
        message: data.message,
        context: data.context,
      };
      const logs = logStore.get(executionId) || [];
      logs.push(logEntry);
      logStore.set(executionId, logs);
      break;

    case 'progress':
      executions.update((map) => {
        const exec = map.get(executionId);
        if (exec) {
          exec.progress = data.percentage;
          exec.currentNode = data.currentNode;
        }
        return map;
      });
      break;

    case 'error':
      executions.update((map) => {
        const exec = map.get(executionId);
        if (exec) {
          exec.error = data.message;
          exec.status = 'failed';
        }
        return map;
      });
      break;

    case 'complete':
      executions.update((map) => {
        const exec = map.get(executionId);
        if (exec) {
          exec.status = data.status;
          exec.endTime = new Date().toISOString();
          exec.output = data.result;
        }
        return map;
      });
      // Unsubscribe after completion
      unsubscribeFromExecution(executionId);
      break;
  }
}

export function getLogs(executionId: string): LogEntry[] {
  return logStore.get(executionId) || [];
}
```

### 2. 实时监控组件

**文件**: `lib/components/ExecutionMonitor.svelte`

```svelte
<script lang="ts">
  import { selectedExecution } from '$lib/stores/executions';
  import { getLogs } from '$lib/api/events';
  import type { Execution } from '$lib/types/execution';

  export let executionId: string;

  let logs = [];
  let autoScroll = true;

  $: execution = $selectedExecution;
  $: logs = getLogs(executionId);

  function handleScroll(e: Event) {
    const el = e.target as HTMLDivElement;
    autoScroll = el.scrollTop + el.clientHeight >= el.scrollHeight - 10;
  }

  function scrollToBottom() {
    if (autoScroll && logsContainer) {
      logsContainer.scrollTop = logsContainer.scrollHeight;
    }
  }

  let logsContainer: HTMLDivElement;

  $: if (logs.length) {
    // Use tick to ensure DOM is updated
    setTimeout(scrollToBottom, 0);
  }
</script>

<div class="monitor">
  <div class="header">
    <h2>执行监控</h2>
    <div class="status" class:running={execution?.status === 'running'}>
      {execution?.status || 'unknown'}
    </div>
  </div>

  <div class="stats">
    <div class="stat">
      <span>进度:</span>
      <span>{execution?.progress || 0}%</span>
    </div>
    <div class="stat">
      <span>当前节点:</span>
      <span>{execution?.currentNode || '-'}</span>
    </div>
    {#if execution?.startTime}
      <div class="stat">
        <span>耗时:</span>
        <span>{calculateDuration(execution.startTime, execution.endTime)}</span>
      </div>
    {/if}
  </div>

  <div class="progress-bar">
    <div
      class="progress-fill"
      style="width: {execution?.progress || 0}%"
    />
  </div>

  <div class="logs-container" bind:this={logsContainer} on:scroll={handleScroll}>
    {#each logs as log (log.timestamp)}
      <div class="log-entry" class:error={log.level === 'error'}>
        <span class="timestamp">{new Date(log.timestamp).toLocaleTimeString()}</span>
        <span class="level" class:debug={log.level === 'debug'}
          class:info={log.level === 'info'}
          class:warn={log.level === 'warn'}
          class:error={log.level === 'error'}>
          {log.level.toUpperCase()}
        </span>
        <span class="message">{log.message}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .monitor {
    display: flex;
    flex-direction: column;
    height: 100%;
    border: 1px solid #e0e0e0;
    border-radius: 8px;
    overflow: hidden;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem;
    border-bottom: 1px solid #e0e0e0;
    background: #f9f9f9;
  }

  .header h2 {
    margin: 0;
    font-size: 1.2rem;
  }

  .status {
    padding: 0.25rem 0.75rem;
    border-radius: 1rem;
    background: #f0f0f0;
    font-size: 0.85rem;
    font-weight: 500;
  }

  .status.running {
    background: #c8e6c9;
    color: #2e7d32;
  }

  .stats {
    display: flex;
    gap: 1.5rem;
    padding: 1rem;
    background: #fafafa;
    border-bottom: 1px solid #e0e0e0;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .stat span:first-child {
    font-size: 0.85rem;
    color: #666;
  }

  .stat span:last-child {
    font-weight: 500;
    font-size: 0.95rem;
  }

  .progress-bar {
    height: 8px;
    background: #e0e0e0;
    position: relative;
  }

  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #4caf50, #45a049);
    transition: width 0.3s ease;
  }

  .logs-container {
    flex: 1;
    overflow-y: auto;
    font-family: 'Monaco', 'Courier New', monospace;
    font-size: 0.85rem;
    padding: 1rem;
    background: #fafafa;
  }

  .log-entry {
    display: flex;
    gap: 1rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid #f0f0f0;
    align-items: flex-start;
  }

  .log-entry.error {
    background: #ffebee;
  }

  .timestamp {
    flex-shrink: 0;
    color: #999;
    min-width: 120px;
  }

  .level {
    flex-shrink: 0;
    font-weight: 600;
    min-width: 60px;
    padding: 0.125rem 0.5rem;
    border-radius: 3px;
    text-align: center;
  }

  .level.debug {
    background: #e3f2fd;
    color: #1976d2;
  }

  .level.info {
    background: #e8f5e9;
    color: #388e3c;
  }

  .level.warn {
    background: #fff3e0;
    color: #f57c00;
  }

  .level.error {
    background: #ffebee;
    color: #d32f2f;
  }

  .message {
    flex: 1;
    word-break: break-word;
    color: #333;
  }
</style>
```

---

## 组件层级

### 页面结构

```
Layout (+layout.svelte)
├─ Header
│  ├─ Logo
│  ├─ Navigation
│  └─ User Menu
├─ Main
│  └─ Routes
│     ├─ Home (+page.svelte)
│     ├─ Workflows
│     │  ├─ List (+page.svelte)
│     │  │  ├─ WorkflowFilters
│     │  │  ├─ WorkflowList
│     │  │  │  └─ WorkflowCard (× N)
│     │  │  └─ Pagination
│     │  └─ Detail ([id]/+page.svelte)
│     │     ├─ WorkflowHeader
│     │     ├─ WorkflowGraph
│     │     └─ Actions
│     └─ Executions
│        ├─ List (+page.svelte)
│        │  ├─ ExecutionFilters
│        │  ├─ ExecutionTable
│        │  └─ Pagination
│        └─ Detail ([id]/+page.svelte)
│           ├─ ExecutionHeader
│           └─ ExecutionMonitor
└─ Footer
```

### 核心组件清单

| 组件 | 用途 | 数据来源 |
|------|------|---------|
| **WorkflowList** | 工作流列表展示 | workflows store |
| **WorkflowCard** | 工作流卡片 | Props |
| **WorkflowGraph** | 流程图可视化 | API getGraph |
| **ExecutionMonitor** | 实时执行监控 | SSE events |
| **ExecutionLog** | 日志展示 | SSE logs |
| **ExecutionStats** | 执行统计 | executions store |

---

## 开发工作流

### 1. 环境设置

```bash
# 安装依赖
cd apps/web-app
pnpm install

# 启动开发服务器（需要后端也在运行）
pnpm dev

# 类型检查
pnpm typecheck

# 代码检查
pnpm lint

# 代码格式化
pnpm format
```

### 2. 开发模式启动脚本

**文件**: 项目根目录 `package.json`

```json
{
  "scripts": {
    "dev:server": "pnpm --filter @wf-agent/server dev",
    "dev:web": "pnpm --filter @wf-agent/web-app dev",
    "dev:all": "concurrently \"pnpm dev:server\" \"pnpm dev:web\""
  }
}
```

启动完整开发环境：
```bash
pnpm dev:all

# 或分开启动
pnpm dev:server  # 后端: http://localhost:3000
pnpm dev:web     # 前端: http://localhost:5173
```

### 3. 调试技巧

#### Network 选项卡
- 检查 API 请求/响应
- 验证 EventSource 连接
- 查看响应时间

#### Console 选项卡
- API 错误日志
- Store 变化日志
- Event 流日志

#### Sources 选项卡
- 断点调试
- 代码覆盖

#### 添加开发日志

```typescript
// lib/api/client.ts
async request<T>(...) {
  console.log(`[API] ${method} ${path}`, options);
  try {
    const response = await fetch(...);
    console.log(`[API] ${response.status}`, data);
    return data.data as T;
  } catch (error) {
    console.error(`[API] Error:`, error);
    throw error;
  }
}
```

---

## 部署

### 1. 构建优化

**文件**: `svelte.config.js`

```javascript
import adapter from '@sveltejs/adapter-auto';

export default {
  kit: {
    adapter: adapter({
      // 自动选择部署适配器
      // Node.js, Vercel, Netlify 等
    }),
    alias: {
      $lib: './src/lib',
    },
  },
};
```

### 2. 生产构建

```bash
pnpm build

# 预览
pnpm preview
```

### 3. 环境管理

**.env.local** (开发)
```
PUBLIC_API_BASE_URL=http://localhost:3000
VITE_LOG_LEVEL=debug
```

**.env.production** (生产)
```
PUBLIC_API_BASE_URL=https://api.example.com
VITE_LOG_LEVEL=error
```

### 4. Docker 部署

**Dockerfile**
```dockerfile
FROM node:22-alpine

WORKDIR /app

# 安装依赖
COPY package.json pnpm-lock.yaml ./
RUN npm install -g pnpm && pnpm install --prod

# 构建
COPY . .
RUN pnpm build

# 运行
EXPOSE 3000
CMD ["node", "build/index.js"]
```

### 5. 部署检查清单

- [ ] 所有环境变量已设置
- [ ] API 基础 URL 正确
- [ ] 构建无错误
- [ ] 类型检查通过
- [ ] Linting 通过
- [ ] 单元测试通过
- [ ] E2E 测试通过
- [ ] 生产构建可预览

---

## 集成时间表

### Phase 4.1: API 客户端 (1-2 天)
- [ ] HTTP 客户端实现
- [ ] API 类型定义
- [ ] 环境配置

### Phase 4.2: Store 层 (1-2 天)
- [ ] Workflow stores
- [ ] Execution stores
- [ ] UI stores

### Phase 4.3: 页面和组件 (3-5 天)
- [ ] 页面结构
- [ ] 工作流管理组件
- [ ] 执行管理组件

### Phase 4.4: 实时通信 (2-3 天)
- [ ] SSE 事件客户端
- [ ] ExecutionMonitor 组件
- [ ] 日志组件

### Phase 4.5: 测试和优化 (2-3 天)
- [ ] 单元测试
- [ ] E2E 测试
- [ ] 性能优化
- [ ] 错误处理完善

**总预计**: 10-15 天（单人开发）

---

## 常见问题

### Q: 如何处理 CORS？
A: 后端已配置 CORS 中间件，允许来自前端的跨域请求。

### Q: SSE 连接断开怎么处理？
A: EventSource API 会自动重连，前端通过 `error` 事件监听断开状态。

### Q: 如何离线工作？
A: 使用 Service Worker + 本地存储缓存数据（Phase 5 计划）。

### Q: 如何处理实时数据冲突？
A: Store 采用 last-write-wins 策略，SSE 事件作为真实源。

---

## 参考资源

- [SvelteKit 文档](https://kit.svelte.dev)
- [Svelte Store](https://svelte.dev/docs/svelte-store)
- [EventSource API](https://developer.mozilla.org/en-US/docs/Web/API/EventSource)
- [Express 文档](https://expressjs.com)

---

**最后更新**: 2026-07-11
**作者**: Claude Code
**状态**: 设计文档完成
