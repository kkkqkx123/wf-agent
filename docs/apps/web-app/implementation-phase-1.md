# Web App 实施方案 - 第一阶段

## 阶段目标

搭建 Web 应用的基础框架,实现核心功能的可用版本,包括工作流管理、线程监控、Agent Loop 交互的基础功能。

## 一、项目结构搭建

### 1.1 创建前端项目

#### 目录结构
```
apps/web-app-frontend/
├── src/
│   ├── lib/
│   │   ├── components/          # UI 组件
│   │   │   ├── common/          # 通用组件
│   │   │   ├── layout/          # 布局组件
│   │   │   ├── workflow/        # 工作流组件
│   │   │   ├── thread/          # 线程组件
│   │   │   └── agent-loop/      # Agent Loop 组件
│   │   ├── stores/              # 状态管理
│   │   │   ├── workflow.ts
│   │   │   ├── thread.ts
│   │   │   ├── agent-loop.ts
│   │   │   └── app.ts
│   │   ├── adapters/            # API 适配器
│   │   │   ├── workflow-adapter.ts
│   │   │   ├── thread-adapter.ts
│   │   │   └── agent-loop-adapter.ts
│   │   ├── services/            # 服务层
│   │   │   ├── api-client.ts
│   │   │   └── websocket-client.ts
│   │   ├── utils/               # 工具函数
│   │   └── types/               # 类型定义
│   ├── routes/                  # SvelteKit 路由
│   │   ├── +layout.svelte
│   │   ├── +page.svelte
│   │   ├── workflows/
│   │   ├── threads/
│   │   └── agent-loops/
│   └── app.html
├── static/
├── package.json
├── svelte.config.js
├── vite.config.ts
└── tsconfig.json
```

#### package.json 配置
```json
{
  "name": "@modular-agent/web-app-frontend",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview",
    "check": "svelte-kit sync && svelte-check --tsconfig ./tsconfig.json",
    "check:watch": "svelte-kit sync && svelte-check --tsconfig ./tsconfig.json --watch"
  },
  "dependencies": {
    "@modular-agent/types": "workspace:*",
    "@modular-agent/common-utils": "workspace:*"
  },
  "devDependencies": {
    "@sveltejs/kit": "^2.0.0",
    "@sveltejs/vite-plugin-svelte": "^3.0.0",
    "svelte": "^5.0.0",
    "typescript": "^5.0.0",
    "vite": "^5.0.0",
    "tailwindcss": "^3.4.0",
    "autoprefixer": "^10.4.0",
    "postcss": "^8.4.0"
  }
}
```

### 1.2 创建后端项目

#### 目录结构
```
apps/web-app-backend/
├── src/
│   ├── routes/                  # REST API 路由
│   │   ├── workflows.ts
│   │   ├── threads.ts
│   │   ├── agent-loops.ts
│   │   └── index.ts
│   ├── sse/                     # SSE 端点处理
│   │   ├── server.ts
│   │   ├── handlers.ts
│   │   └── types.ts
│   ├── adapters/                # SDK 适配器
│   │   ├── workflow-adapter.ts
│   │   ├── thread-adapter.ts
│   │   └── agent-loop-adapter.ts
│   ├── middleware/              # 中间件
│   │   ├── error-handler.ts
│   │   ├── logger.ts
│   │   └── cors.ts
│   ├── utils/                   # 工具函数
│   └── index.ts                 # 入口文件
├── package.json
└── tsconfig.json
```

#### package.json 配置
```json
{
  "name": "@modular-agent/web-app-backend",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "dev": "tsx watch src/index.ts",
    "build": "tsc",
    "start": "node dist/index.js"
  },
  "dependencies": {
    "@modular-agent/sdk": "workspace:*",
    "@modular-agent/types": "workspace:*",
    "@modular-agent/common-utils": "workspace:*",
    "express": "^4.18.2"
  },
  "devDependencies": {
    "@types/express": "^4.17.21",
    "tsx": "^4.7.0",
    "typescript": "^5.0.0"
  }
}
```

## 二、前端核心实现

### 2.1 布局组件

#### 主布局 (src/lib/components/layout/MainLayout.svelte)
```svelte
<script lang="ts">
  import Sidebar from './Sidebar.svelte';
  import Header from './Header.svelte';
  
  let { children } = $props();
</script>

<div class="flex h-screen bg-gray-100">
  <Sidebar />
  <div class="flex-1 flex flex-col">
    <Header />
    <main class="flex-1 overflow-auto p-6">
      {@render children()}
    </main>
  </div>
</div>
```

#### 侧边栏 (src/lib/components/layout/Sidebar.svelte)
```svelte
<script lang="ts">
  import { page } from '$app/stores';
  
  const menuItems = [
    { path: '/workflows', label: '工作流', icon: 'workflow' },
    { path: '/threads', label: '线程', icon: 'thread' },
    { path: '/agent-loops', label: 'Agent Loop', icon: 'agent' },
  ];
</script>

<aside class="w-64 bg-white shadow-lg">
  <nav class="p-4">
    <ul class="space-y-2">
      {#each menuItems as item}
        <li>
          <a 
            href={item.path}
            class="flex items-center px-4 py-2 rounded-lg hover:bg-gray-100"
            class:bg-blue-100={$page.url.pathname.startsWith(item.path)}
          >
            <span>{item.label}</span>
          </a>
        </li>
      {/each}
    </ul>
  </nav>
</aside>
```

### 2.2 状态管理

#### 工作流 Store (src/lib/stores/workflow.ts)
```typescript
import { writable, derived } from 'svelte/store';
import type { Workflow } from '@modular-agent/types';

interface WorkflowState {
  workflows: Workflow[];
  currentWorkflow: Workflow | null;
  loading: boolean;
  error: string | null;
}

function createWorkflowStore() {
  const { subscribe, set, update } = writable<WorkflowState>({
    workflows: [],
    currentWorkflow: null,
    loading: false,
    error: null,
  });

  return {
    subscribe,
    setWorkflows: (workflows: Workflow[]) => 
      update(s => ({ ...s, workflows, loading: false })),
    setCurrentWorkflow: (workflow: Workflow | null) => 
      update(s => ({ ...s, currentWorkflow: workflow })),
    setLoading: (loading: boolean) => 
      update(s => ({ ...s, loading })),
    setError: (error: string | null) => 
      update(s => ({ ...s, error, loading: false })),
  };
}

export const workflowStore = createWorkflowStore();
```

#### 线程 Store (src/lib/stores/thread.ts)
```typescript
import { writable } from 'svelte/store';
import type { Thread } from '@modular-agent/types';

interface ThreadState {
  threads: Thread[];
  currentThread: Thread | null;
  loading: boolean;
  error: string | null;
}

function createThreadStore() {
  const { subscribe, set, update } = writable<ThreadState>({
    threads: [],
    currentThread: null,
    loading: false,
    error: null,
  });

  return {
    subscribe,
    setThreads: (threads: Thread[]) => 
      update(s => ({ ...s, threads, loading: false })),
    setCurrentThread: (thread: Thread | null) => 
      update(s => ({ ...s, currentThread: thread })),
    updateThread: (id: string, updates: Partial<Thread>) =>
      update(s => ({
        ...s,
        threads: s.threads.map(t => t.id === id ? { ...t, ...updates } : t),
      })),
    setLoading: (loading: boolean) => 
      update(s => ({ ...s, loading })),
    setError: (error: string | null) => 
      update(s => ({ ...s, error, loading: false })),
  };
}

export const threadStore = createThreadStore();
```

### 2.3 API 适配器

#### 工作流适配器 (src/lib/adapters/workflow-adapter.ts)
```typescript
import { apiClient } from '$lib/services/api-client';
import type { Workflow } from '@modular-agent/types';

export class WorkflowAdapter {
  async listWorkflows(): Promise<Workflow[]> {
    const response = await apiClient.get<Workflow[]>('/api/workflows');
    return response.data;
  }

  async getWorkflow(id: string): Promise<Workflow> {
    const response = await apiClient.get<Workflow>(`/api/workflows/${id}`);
    return response.data;
  }

  async registerFromFile(file: File, params?: Record<string, unknown>): Promise<Workflow> {
    const formData = new FormData();
    formData.append('file', file);
    if (params) {
      formData.append('params', JSON.stringify(params));
    }
    const response = await apiClient.post<Workflow>('/api/workflows/register-file', formData);
    return response.data;
  }

  async deleteWorkflow(id: string): Promise<void> {
    await apiClient.delete(`/api/workflows/${id}`);
  }
}
```

#### 线程适配器 (src/lib/adapters/thread-adapter.ts)
```typescript
import { apiClient } from '$lib/services/api-client';
import type { Thread } from '@modular-agent/types';

export class ThreadAdapter {
  async listThreads(): Promise<Thread[]> {
    const response = await apiClient.get<Thread[]>('/api/threads');
    return response.data;
  }

  async getThread(id: string): Promise<Thread> {
    const response = await apiClient.get<Thread>(`/api/threads/${id}`);
    return response.data;
  }

  async executeThread(workflowId: string, input: Record<string, unknown>): Promise<Thread> {
    const response = await apiClient.post<Thread>(`/api/threads/${workflowId}/execute`, { input });
    return response.data;
  }

  async pauseThread(id: string): Promise<void> {
    await apiClient.post(`/api/threads/${id}/pause`);
  }

  async resumeThread(id: string): Promise<void> {
    await apiClient.post(`/api/threads/${id}/resume`);
  }

  async cancelThread(id: string): Promise<void> {
    await apiClient.post(`/api/threads/${id}/cancel`);
  }
}
```

### 2.4 SSE 客户端

#### SSE 服务 (src/lib/services/sse-client.ts)
```typescript
import { writable } from 'svelte/store';

interface SSEMessage {
  type: string;
  data: any;
}

export class SSEClient {
  private eventSource: EventSource | null = null;
  
  public connected = writable(false);
  public messages = writable<SSEMessage[]>([]);

  connect(url: string, eventTypes: string[] = []) {
    this.eventSource = new EventSource(url);
    
    this.eventSource.onopen = () => {
      this.connected.set(true);
      console.log('SSE connected');
    };

    // 监听所有事件类型
    eventTypes.forEach(eventType => {
      this.eventSource.addEventListener(eventType, (event) => {
        const message: SSEMessage = {
          type: eventType,
          data: JSON.parse(event.data)
        };
        this.messages.update(msgs => [...msgs, message]);
      });
    });

    this.eventSource.onerror = (error) => {
      console.error('SSE error:', error);
      // EventSource 会自动重连
    };
  }

  subscribe(eventType: string, callback: (data: any) => void) {
    return this.messages.subscribe(msgs => {
      const relevantMsgs = msgs.filter(m => m.type === eventType);
      relevantMsgs.forEach(m => callback(m.data));
    });
  }

  disconnect() {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }
}

export const sseClient = new SSEClient();
```

### 2.5 页面实现

#### 工作流列表页 (src/routes/workflows/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { workflowStore } from '$lib/stores/workflow';
  import { WorkflowAdapter } from '$lib/adapters/workflow-adapter';
  import WorkflowCard from '$lib/components/workflow/WorkflowCard.svelte';

  const adapter = new WorkflowAdapter();
  
  onMount(async () => {
    workflowStore.setLoading(true);
    try {
      const workflows = await adapter.listWorkflows();
      workflowStore.setWorkflows(workflows);
    } catch (error) {
      workflowStore.setError(error.message);
    }
  });
</script>

{#if $workflowStore.loading}
  <div class="flex justify-center items-center h-64">
    <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
  </div>
{:else if $workflowStore.error}
  <div class="text-red-500">{$workflowStore.error}</div>
{:else}
  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
    {#each $workflowStore.workflows as workflow}
      <WorkflowCard {workflow} />
    {/each}
  </div>
{/if}
```

#### 线程监控页 (src/routes/threads/[id]/+page.svelte)
```svelte
<script lang="ts">
  import { page } from '$app/stores';
  import { onMount, onDestroy } from 'svelte';
  import { threadStore } from '$lib/stores/thread';
  import { ThreadAdapter } from '$lib/adapters/thread-adapter';
  import { sseClient } from '$lib/services/sse-client';
  import ThreadMonitor from '$lib/components/thread/ThreadMonitor.svelte';

  const adapter = new ThreadAdapter();
  const threadId = $page.params.id;
  
  let unsubscribe: (() => void) | null = null;

  onMount(async () => {
    // Load thread data
    threadStore.setLoading(true);
    try {
      const thread = await adapter.getThread(threadId);
      threadStore.setCurrentThread(thread);
    } catch (error) {
      threadStore.setError(error.message);
    }

    // Subscribe to thread events via SSE
    sseClient.connect(`/api/threads/${threadId}/events`, ['progress', 'node:executed', 'error']);
    unsubscribe = sseClient.subscribe('progress', (data) => {
      if (data.threadId === threadId) {
        threadStore.updateThread(threadId, data);
      }
    });
  });

  onDestroy(() => {
    if (unsubscribe) unsubscribe();
    sseClient.disconnect();
  });
</script>

{#if $threadStore.currentThread}
  <ThreadMonitor thread={$threadStore.currentThread} />
{:else}
  <div>Loading...</div>
{/if}
```

## 三、后端核心实现

### 3.1 REST API 路由

#### 工作流路由 (src/routes/workflows.ts)
```typescript
import { Router } from 'express';
import { WorkflowAdapter } from '../adapters/workflow-adapter';

const router = Router();
const adapter = new WorkflowAdapter();

// List workflows
router.get('/', async (req, res) => {
  try {
    const workflows = await adapter.listWorkflows();
    res.json(workflows);
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

// Get workflow by ID
router.get('/:id', async (req, res) => {
  try {
    const workflow = await adapter.getWorkflow(req.params.id);
    res.json(workflow);
  } catch (error) {
    res.status(404).json({ error: 'Workflow not found' });
  }
});

// Register from file
router.post('/register-file', async (req, res) => {
  try {
    const file = req.file;
    const params = req.body.params ? JSON.parse(req.body.params) : undefined;
    const workflow = await adapter.registerFromFile(file.path, params);
    res.json(workflow);
  } catch (error) {
    res.status(400).json({ error: error.message });
  }
});

// Delete workflow
router.delete('/:id', async (req, res) => {
  try {
    await adapter.deleteWorkflow(req.params.id);
    res.status(204).send();
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

export default router;
```

#### 线程路由 (src/routes/threads.ts)
```typescript
import { Router } from 'express';
import { ThreadAdapter } from '../adapters/thread-adapter';

const router = Router();
const adapter = new ThreadAdapter();

// List threads
router.get('/', async (req, res) => {
  try {
    const threads = await adapter.listThreads();
    res.json(threads);
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

// Get thread by ID
router.get('/:id', async (req, res) => {
  try {
    const thread = await adapter.getThread(req.params.id);
    res.json(thread);
  } catch (error) {
    res.status(404).json({ error: 'Thread not found' });
  }
});

// Execute thread
router.post('/:workflowId/execute', async (req, res) => {
  try {
    const { input } = req.body;
    const thread = await adapter.executeThread(req.params.workflowId, input);
    res.json(thread);
  } catch (error) {
    res.status(400).json({ error: error.message });
  }
});

// Pause thread
router.post('/:id/pause', async (req, res) => {
  try {
    await adapter.pauseThread(req.params.id);
    res.status(204).send();
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

// Resume thread
router.post('/:id/resume', async (req, res) => {
  try {
    await adapter.resumeThread(req.params.id);
    res.status(204).send();
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

// Cancel thread
router.post('/:id/cancel', async (req, res) => {
  try {
    await adapter.cancelThread(req.params.id);
    res.status(204).send();
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

export default router;
```

### 3.2 SSE 端点

#### SSE 服务 (src/sse/server.ts)
```typescript
import { EventEmitter } from 'events';

export class SSEManager extends EventEmitter {
  private clients: Map<string, {
    response: any;
    eventTypes: string[];
  }> = new Map();

  constructor() {
    super();
  }

  registerClient(clientId: string, response: any, eventTypes: string[] = []) {
    this.clients.set(clientId, { response, eventTypes });
    console.log(`SSE client connected: ${clientId}`);

    // 发送连接成功事件
    this.sendTo(clientId, 'connected', { clientId });
  }

  unregisterClient(clientId: string) {
    this.clients.delete(clientId);
    console.log(`SSE client disconnected: ${clientId}`);
  }

  sendTo(clientId: string, eventType: string, data: any) {
    const client = this.clients.get(clientId);
    if (!client) return;

    const eventData = JSON.stringify(data);
    client.response.write(`event: ${eventType}\ndata: ${eventData}\n\n`);
  }

  broadcast(eventType: string, data: any) {
    this.clients.forEach((client, clientId) => {
      if (client.eventTypes.length === 0 || client.eventTypes.includes(eventType)) {
        this.sendTo(clientId, eventType, data);
      }
    });
  }

  get connectedClients(): number {
    return this.clients.size;
  }
}

export const sseManager = new SSEManager();
```

### 3.3 SDK 适配器

#### 工作流适配器 (src/adapters/workflow-adapter.ts)
```typescript
import { getSDK } from '@modular-agent/sdk';
import type { Workflow } from '@modular-agent/types';

export class WorkflowAdapter {
  private sdk = getSDK();

  async listWorkflows(): Promise<Workflow[]> {
    const api = this.sdk.getAPI('workflow');
    const result = await api.list();
    return result.data;
  }

  async getWorkflow(id: string): Promise<Workflow> {
    const api = this.sdk.getAPI('workflow');
    const result = await api.get(id);
    return result.data;
  }

  async registerFromFile(filePath: string, params?: Record<string, unknown>): Promise<Workflow> {
    const api = this.sdk.getAPI('workflow');
    const result = await api.registerFromFile(filePath, params);
    return result.data;
  }

  async deleteWorkflow(id: string): Promise<void> {
    const api = this.sdk.getAPI('workflow');
    await api.delete(id);
  }
}
```

#### 线程适配器 (src/adapters/thread-adapter.ts)
```typescript
import { getSDK } from '@modular-agent/sdk';
import type { Thread } from '@modular-agent/types';

export class ThreadAdapter {
  private sdk = getSDK();

  async listThreads(): Promise<Thread[]> {
    const api = this.sdk.getAPI('thread');
    const result = await api.list();
    return result.data;
  }

  async getThread(id: string): Promise<Thread> {
    const api = this.sdk.getAPI('thread');
    const result = await api.get(id);
    return result.data;
  }

  async executeThread(workflowId: string, input: Record<string, unknown>): Promise<Thread> {
    const command = this.sdk.createCommand('executeThread', {
      workflowId,
      input,
    });
    const result = await this.sdk.execute(command);
    return result.data;
  }

  async pauseThread(id: string): Promise<void> {
    const command = this.sdk.createCommand('pauseThread', { threadId: id });
    await this.sdk.execute(command);
  }

  async resumeThread(id: string): Promise<void> {
    const command = this.sdk.createCommand('resumeThread', { threadId: id });
    await this.sdk.execute(command);
  }

  async cancelThread(id: string): Promise<void> {
    const command = this.sdk.createCommand('cancelThread', { threadId: id });
    await this.sdk.execute(command);
  }
}
```

### 3.4 主入口文件

#### 入口文件 (src/index.ts)
```typescript
import express from 'express';
import cors from 'cors';
import { sseManager } from './sse/server';
import workflowsRouter from './routes/workflows';
import threadsRouter from './routes/threads';
import agentLoopsRouter from './routes/agent-loops';
import { errorHandler } from './middleware/error-handler';
import { logger } from './middleware/logger';

const app = express();
const PORT = process.env.PORT || 3000;

// Middleware
app.use(cors());
app.use(express.json());
app.use(logger);

// Routes
app.use('/api/workflows', workflowsRouter);
app.use('/api/threads', threadsRouter);
app.use('/api/agent-loops', agentLoopsRouter);

// Error handler
app.use(errorHandler);

// Start HTTP server
app.listen(PORT, () => {
  console.log(`HTTP server running on port ${PORT}`);
  console.log(`SSE endpoints available on same port`);
});

// SSE 事件可以通过 sseManager.broadcast() 推送
```

## 四、配置文件

### 4.1 SvelteKit 配置

#### svelte.config.js
```javascript
import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({
      out: 'build',
      precompress: false,
    }),
  },
};

export default config;
```

### 4.2 Vite 配置

#### vite.config.ts
```typescript
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:3000',
        changeOrigin: true,
      },
    },
  },
});
```

### 4.3 TailwindCSS 配置

#### tailwind.config.js
```javascript
/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {},
  },
  plugins: [],
};
```

## 五、开发任务清单

### 5.1 前端任务
- [ ] 创建 SvelteKit 项目结构
- [ ] 配置 TailwindCSS
- [ ] 实现布局组件(Header、Sidebar、MainLayout)
- [ ] 实现通用组件(Button、Input、Modal、Table、Card)
- [ ] 实现状态管理(WorkflowStore、ThreadStore、AgentLoopStore)
- [ ] 实现 API 客户端
- [ ] 实现 SSE 客户端
- [ ] 实现工作流列表页
- [ ] 实现工作流详情页
- [ ] 实现线程列表页
- [ ] 实现线程监控页
- [ ] 实现 Agent Loop 列表页
- [ ] 实现 Agent Loop 对话页

### 5.2 后端任务
- [ ] 创建 Express 项目结构
- [ ] 实现中间件(cors、error-handler、logger)
- [ ] 实现工作流路由
- [ ] 实现线程路由
- [ ] 实现 Agent Loop 路由
- [ ] 实现 SSE 端点
- [ ] 实现 SDK 适配器(Workflow、Thread、AgentLoop)
- [ ] 集成 SDK 事件系统
- [ ] 实现事件推送机制

### 5.3 集成任务
- [ ] 配置前端代理
- [ ] 测试前后端通信
- [ ] 测试 SSE 端点连接
- [ ] 测试实时事件推送
- [ ] 编写集成测试

## 六、验收标准

### 6.1 功能验收
- ✅ 能够查看工作流列表
- ✅ 能够查看工作流详情
- ✅ 能够从文件注册工作流
- ✅ 能够删除工作流
- ✅ 能够查看线程列表
- ✅ 能够查看线程详情
- ✅ 能够执行线程
- ✅ 能够暂停/恢复/取消线程
- ✅ 能够实时监控线程执行
- ✅ 能够查看 Agent Loop 列表
- ✅ 能够与 Agent Loop 对话
- ✅ 能够查看消息流

### 6.2 技术验收
- ✅ 前端项目可独立运行
- ✅ 后端项目可独立运行
- ✅ 前后端可正常通信
- ✅ SSE 端点连接稳定
- ✅ 实时事件推送正常
- ✅ 代码类型检查通过
- ✅ 无明显性能问题

## 七、时间估算

- 项目结构搭建: 1 天
- 前端基础组件: 2 天
- 前端状态管理和适配器: 2 天
- 前端页面实现: 3 天
- 后端路由和适配器: 2 天
- SSE 端点实现: 1 天
- 集成测试: 2 天
- **总计: 约 13 天**

## 八、风险与应对

### 8.1 技术风险
- **WebSocket 连接稳定性**: 实现自动重连机制
- **实时事件推送性能**: 使用消息队列缓冲
- **前后端类型一致性**: 共享类型定义包

### 8.2 集成风险
- **SDK API 变更**: 保持适配器层隔离
- **事件系统复杂度**: 简化事件类型,逐步扩展

## 九、下一步计划

完成第一阶段后,进入第二阶段:
- 实现工作流可视化编辑器
- 实现线程执行流程可视化
- 实现资源管理功能
- 实现检查点管理
- 实现事件监控
