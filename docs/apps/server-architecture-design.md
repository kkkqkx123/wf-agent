# 统一 Server 架构设计

基于 CLI-app 的多适配器架构分析，本文档设计一个 Web 前端和 VSCode WebView 前端共用的通用 Server。

## 1. 概述

### 背景

当前项目有三个前端界面：
- **CLI App**: 命令行界面，通过 Commander.js 和 TUI 提供交互
- **Web App**: Web 浏览器前端，基于 Svelte
- **VSCode App**: 编辑器扩展，包含 WebView 子窗口

CLI-app 使用多适配器模式，为不同的资源类型（Workflow、Tool、Template 等）提供统一的查询、操作接口。Web 和 VSCode 两个前端需要通过 HTTP API 与后端交互，因此需要一个通用的 Server。

### 目标

设计一个统一的 Server，满足：
1. **跨前端共用**: Web 浏览器和 VSCode WebView 共用同一套 API
2. **多通信协议**: 支持 REST（查询/操作）、WebSocket（实时流）、SSE（Server-Sent Events）
3. **适配器复用**: 复用 CLI-app 的 Adapter 模式，统一资源访问接口
4. **SDK 集成**: 完整初始化和管理 SDK 实例的生命周期
5. **可扩展性**: 易于扩展新的 API 端点和资源类型

## 2. 系统架构

### 2.1 整体分层

```
┌────────────────────────────────────────────────────────────┐
│                   Frontend Layer                           │
│  ┌──────────────┐              ┌──────────────┐           │
│  │  Web App     │              │  VSCode      │           │
│  │  (Svelte)    │              │  WebView     │           │
│  └──────────────┘              └──────────────┘           │
├────────────────────────────────────────────────────────────┤
│                  Communication Layer                        │
│  ┌────────────────────────────────────────────────────┐    │
│  │ HTTP + WebSocket + SSE                             │    │
│  │ (Unified API Contract)                             │    │
│  └────────────────────────────────────────────────────┘    │
├────────────────────────────────────────────────────────────┤
│              Unified Server (Node.js)                       │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              API Layer                              │  │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐   │  │
│  │  │  Routes     │ │ Middleware  │ │ Auth/CORS   │   │  │
│  │  └─────────────┘ └─────────────┘ └─────────────┘   │  │
│  └──────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │           Adapter Layer (Unified Interface)         │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │  │
│  │  │Workflow  │ │  Tool    │ │ Template │ ...        │  │
│  │  │Adapter   │ │ Adapter  │ │ Adapter  │            │  │
│  │  └──────────┘ └──────────┘ └──────────┘            │  │
│  └──────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │          Service Layer                              │  │
│  │  ┌──────────────┐  ┌──────────────┐               │  │
│  │  │ExecutionSvc  │  │EventBus Svc  │ ...          │  │
│  │  └──────────────┘  └──────────────┘               │  │
│  └──────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Core SDK Layer                          │  │
│  │  ┌──────────────────────────────────────────────┐   │  │
│  │  │  SDK Instance (Singleton)                    │   │  │
│  │  │  - Storage Adapters                          │   │  │
│  │  │  - Configuration Loading                     │   │  │
│  │  │  - Workflow Execution Engine                 │   │  │
│  │  │  - LLM/Tool Integration                      │   │  │
│  │  └──────────────────────────────────────────────┘   │  │
│  └──────────────────────────────────────────────────────┘  │
├────────────────────────────────────────────────────────────┤
│                    Storage Layer                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ File/Database Storage (via SDK)                      │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

### 2.2 模块职责

| 模块 | 职责 | 依赖 |
|-----|------|------|
| **API Routes** | HTTP 路由定义、请求路由、响应格式化 | Adapters |
| **Adapter Layer** | 统一的资源查询和操作接口 | SDK, Services |
| **Service Layer** | 业务逻辑（执行、事件、监控等） | SDK |
| **SDK Core** | 工作流执行、存储、LLM集成 | 配置、存储驱动 |
| **Middleware** | 认证、错误处理、请求日志、CORS | 无 |

## 3. API 设计

### 3.1 API 基础

**基 URL**: `http://localhost:3000/api/v1`

**协议**:
- **REST**: 查询、创建、删除操作
- **WebSocket** (`/ws`): 实时事件流（Workflow 执行、日志更新等）
- **SSE** (`/events`): 另一套实时事件机制（如果不用 WebSocket）

### 3.2 核心资源端点

#### Workflow API

```
GET    /workflows              # 列表所有工作流
POST   /workflows              # 创建工作流
GET    /workflows/:id          # 获取工作流详情
PUT    /workflows/:id          # 更新工作流
DELETE /workflows/:id          # 删除工作流
POST   /workflows/:id/execute  # 执行工作流
GET    /workflows/:id/graph    # 获取工作流图结构
```

#### Workflow Execution API

```
GET    /executions                    # 列表所有执行
GET    /executions/:executionId       # 获取执行详情
POST   /executions/:executionId/pause # 暂停执行
POST   /executions/:executionId/resume # 恢复执行
POST   /executions/:executionId/stop  # 停止执行
GET    /executions/:executionId/logs  # 获取执行日志
```

#### Tool API

```
GET    /tools                # 列表所有工具
GET    /tools/:id            # 获取工具详情
POST   /tools/:id/validate   # 验证工具配置
```

#### Template API

```
GET    /templates            # 列表所有模板
GET    /templates/:id        # 获取模板详情
POST   /templates/:id/instantiate # 从模板创建工作流
```

#### Checkpoint API

```
GET    /checkpoints/:executionId     # 列表执行的检查点
GET    /checkpoints/:checkpointId    # 获取检查点详情
POST   /checkpoints/:checkpointId/restore # 恢复检查点
```

#### Event Streaming

```
WebSocket ws://localhost:3000/ws
  连接后发送: { type: "subscribe", resource: "execution", id: ":executionId" }
  接收: { type: "update", resource: "execution", data: {...} }

或

SSE GET /events?resource=execution&id=:executionId
  事件流: data: {"type": "update", ...}
```

### 3.3 请求/响应格式

**标准响应**:
```json
{
  "success": true,
  "data": { /* 实际数据 */ },
  "meta": { "timestamp": "2024-07-11T10:00:00Z" }
}
```

**错误响应**:
```json
{
  "success": false,
  "error": {
    "code": "WORKFLOW_NOT_FOUND",
    "message": "工作流不存在",
    "details": {}
  }
}
```

## 4. 服务器实现

### 4.1 技术栈

- **Runtime**: Node.js ≥22.0.0
- **Web Framework**: Express.js 或 Fastify
- **WebSocket**: ws 库（或 Socket.io）
- **EventBus**: RxJS 或原生 EventEmitter
- **TypeScript**: 完整类型支持
- **Dependencies**: @wf-agent/sdk, @wf-agent/config-processor, @wf-agent/storage

### 4.2 核心类结构

#### Server 类

```typescript
class Server {
  private express: Express;
  private sdkInstance: SDKInstance;
  private httpServer: http.Server;
  private wsServer: WebSocketServer;
  private container: ServerDependencyContainer;
  
  async bootstrap(): Promise<void>
  async start(port: number): Promise<void>
  async shutdown(): Promise<void>
}
```

#### ServerDependencyContainer

参考 CLI-app 的 CLIDependencyContainer，集中管理依赖：

```typescript
class ServerDependencyContainer {
  private sdk: SDKInstance;
  private adapters: Map<string, BaseAdapter>;
  private services: Map<string, BaseService>;
  private eventBus: EventBus;
  
  getSDK(): SDKInstance
  getAdapter<T>(name: string): T
  getService<T>(name: string): T
  getEventBus(): EventBus
}
```

#### 适配器（复用 CLI-app）

继承自 CLI-app 中的 BaseAdapter：

```typescript
class WorkflowAdapter extends BaseAdapter {
  async list(query: QueryOptions): Promise<Workflow[]>
  async get(id: string): Promise<Workflow>
  async create(data: WorkflowInput): Promise<Workflow>
  async update(id: string, data: Partial<WorkflowInput>): Promise<Workflow>
  async delete(id: string): Promise<void>
  async getGraph(id: string): Promise<WorkflowGraph>
}

class ToolAdapter extends BaseAdapter {
  async list(query: QueryOptions): Promise<Tool[]>
  async get(id: string): Promise<Tool>
  async validate(id: string, config: any): Promise<ValidationResult>
}

// 其他适配器类似...
```

#### 服务层

```typescript
class ExecutionService {
  async execute(workflowId: string, input: any): Promise<ExecutionId>
  async getStatus(executionId: string): Promise<ExecutionStatus>
  async pause(executionId: string): Promise<void>
  async resume(executionId: string): Promise<void>
  async stop(executionId: string): Promise<void>
  onExecutionUpdate(callback: (update: ExecutionUpdate) => void): Unsubscribe
}

class EventBusService {
  subscribe(topic: string, handler: Handler): Unsubscribe
  publish(topic: string, event: Event): void
}
```

### 4.3 初始化流程

```typescript
async function bootstrap() {
  // 1. 加载配置
  const config = await loadConfigWithEnvOverride();
  
  // 2. 初始化 SDK（与 CLI-app 相同）
  const sdk = createSDK({
    debug: config.debug,
    logging: config.logging,
    ...storageManager.getAllAdapters(),
  });
  await sdk.waitForReady();
  registerAllIndexResolvers();
  
  // 3. 初始化容器
  const container = new ServerDependencyContainer(sdk, config);
  
  // 4. 注册适配器
  container.registerAdapter('workflow', new WorkflowAdapter(sdk));
  container.registerAdapter('tool', new ToolAdapter(sdk));
  // ...
  
  // 5. 注册服务
  container.registerService('execution', new ExecutionService(sdk));
  container.registerService('event', new EventBusService());
  // ...
  
  // 6. 启动 Server
  const server = new Server(container, config);
  await server.start(config.port || 3000);
}
```

### 4.4 路由定义示例

```typescript
// routes/workflow.ts
export function createWorkflowRoutes(router: Router, container: ServerDependencyContainer) {
  const adapter = container.getAdapter<WorkflowAdapter>('workflow');
  
  router.get('/', async (req, res) => {
    try {
      const workflows = await adapter.list(req.query);
      res.json({
        success: true,
        data: workflows,
      });
    } catch (error) {
      res.status(500).json({
        success: false,
        error: { code: 'LIST_ERROR', message: error.message },
      });
    }
  });
  
  router.post('/:id/execute', async (req, res) => {
    try {
      const service = container.getService<ExecutionService>('execution');
      const executionId = await service.execute(req.params.id, req.body);
      res.json({ success: true, data: { executionId } });
    } catch (error) {
      // 错误处理
    }
  });
}
```

## 5. 前端集成

### 5.1 API Client 库

创建 `@wf-agent/server-api-client` 包（可选但推荐）：

```typescript
export class APIClient {
  constructor(baseURL: string, auth?: AuthToken) { }
  
  workflows: WorkflowAPI
  executions: ExecutionAPI
  tools: ToolAPI
  templates: TemplateAPI
  // ...
  
  onConnect(handler: () => void): Unsubscribe
  onDisconnect(handler: () => void): Unsubscribe
}

export class WorkflowAPI {
  list(query?: QueryOptions): Promise<Workflow[]>
  get(id: string): Promise<Workflow>
  execute(id: string, input: any): Promise<{ executionId: string }>
  // ...
}
```

### 5.2 Web App 集成

在 `apps/web-app-frontend/src` 中：

```typescript
// src/lib/api/client.ts
import { APIClient } from '@wf-agent/server-api-client';

export const apiClient = new APIClient(
  import.meta.env.VITE_API_BASE_URL || 'http://localhost:3000/api/v1'
);

// src/stores/workflows.ts
import { readable } from 'svelte/store';

export const workflows = readable([], async (set) => {
  const data = await apiClient.workflows.list();
  set(data);
});
```

### 5.3 VSCode WebView 集成

在 `apps/vscode-app/src` 中（WebView 代码）：

```typescript
// src/webview/api/client.ts
import { APIClient } from '@wf-agent/server-api-client';

// 通过 vscode.postMessage 与 extension 后端通信
// extension 后端连接 Server 并转发请求
export const apiClient = new APIClient(/* config */);
```

## 6. 生命周期管理

### 6.1 启动流程

```
1. 解析命令行参数和环境变量
2. 加载配置文件（.modular-agent.toml 或环境变量）
3. 初始化日志系统
4. 初始化存储管理器
5. 创建 SDK 实例并等待就绪
6. 注册配置索引解析器
7. 初始化依赖容器
8. 注册所有适配器和服务
9. 启动 HTTP Server + WebSocket Server
10. 监听指定端口
```

### 6.2 优雅关闭

```typescript
async function gracefulShutdown() {
  console.log('开始优雅关闭...');
  
  // 1. 停止接收新连接
  httpServer.close();
  wsServer.close();
  
  // 2. 等待正在进行的执行完成或超时
  await sdk.gracefulShutdown(15000); // 15 秒超时
  
  // 3. 关闭存储连接
  await storageManager.close();
  
  console.log('Server 已关闭');
  process.exit(0);
}

process.on('SIGTERM', gracefulShutdown);
process.on('SIGINT', gracefulShutdown);
```

## 7. 错误处理和日志

### 7.1 错误分类

| 错误类型 | HTTP 状态码 | 说明 |
|---------|-----------|------|
| `RESOURCE_NOT_FOUND` | 404 | 资源不存在 |
| `VALIDATION_ERROR` | 400 | 请求数据验证失败 |
| `UNAUTHORIZED` | 401 | 认证失败 |
| `FORBIDDEN` | 403 | 无权限访问 |
| `CONFLICT` | 409 | 资源冲突（如已存在） |
| `INTERNAL_ERROR` | 500 | 服务器内部错误 |

### 7.2 日志策略

- **Access Log**: 记录所有 HTTP 请求（路由、耗时、状态码）
- **API Log**: 适配器和服务调用日志
- **SDK Log**: SDK 内部日志（工作流执行等）
- **Error Log**: 所有错误堆栈和上下文
- **Event Log**: 重要事件时间戳（启动、执行、关闭等）

## 8. 扩展性和最佳实践

### 8.1 添加新的适配器

```typescript
// adapters/custom-adapter.ts
export class CustomAdapter extends BaseAdapter {
  constructor(private sdk: SDKInstance) {}
  
  async list(query: QueryOptions) {
    // 实现列表逻辑
  }
}

// 在 bootstrap 中注册
container.registerAdapter('custom', new CustomAdapter(sdk));
```

### 8.2 添加新的 API 端点

```typescript
// routes/custom.ts
export function createCustomRoutes(
  router: Router,
  container: ServerDependencyContainer
) {
  router.get('/resource', async (req, res) => {
    const adapter = container.getAdapter<CustomAdapter>('custom');
    // 使用适配器处理请求
  });
}

// 在 server bootstrap 中：
app.use('/api/v1/custom', createCustomRoutes(Router(), container));
```

### 8.3 性能优化

- **缓存**: Redis 或内存缓存工作流/工具列表
- **分页**: API 端点支持 offset/limit 分页
- **索引**: 数据库索引优化查询性能
- **连接池**: 如使用数据库，配置连接池
- **异步处理**: 后台任务（日志归档、清理）

## 9. 部署和配置

### 9.1 环境变量

```bash
# Server 配置
SERVER_PORT=3000
SERVER_HOST=0.0.0.0

# SDK 配置
SDK_DEBUG=false
SDK_LOG_LEVEL=info

# 存储配置
STORAGE_TYPE=file  # file | database
STORAGE_PATH=./data

# 可选：前端地址（CORS）
CORS_ORIGINS=http://localhost:5173,http://localhost:8080
```

### 9.2 Docker 部署示例

```dockerfile
FROM node:22-alpine
WORKDIR /app
COPY . .
RUN pnpm install --prod
RUN pnpm --filter @wf-agent/server build
EXPOSE 3000
CMD ["node", "apps/server/dist/index.js"]
```

## 10. 总结

该架构通过以下方式实现统一的 Server：

1. **适配器模式**: 复用 CLI-app 的 Adapter 架构，统一资源访问
2. **依赖注入**: 通过容器管理依赖，提高可测试性和可维护性
3. **多通信协议**: 同时支持 REST、WebSocket、SSE，满足不同使用场景
4. **完整的 SDK 集成**: 完整初始化和管理 SDK 生命周期
5. **清晰的分层**: API 层、适配器层、服务层、SDK 核心层清晰分离
6. **易于扩展**: 新的资源类型或端点易于添加

此架构既能满足 Web 和 VSCode 前端的共用需求，也为未来集成其他客户端（移动 App、桌面 App 等）提供了基础。
