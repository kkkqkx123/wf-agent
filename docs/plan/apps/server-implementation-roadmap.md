# Server 统一实现路线图

## 概述

本文档定义了将架构设计转化为可工作的 Server 的分阶段实现计划。该 Server 将为 Web 前端和 VSCode WebView 前端提供统一的 API。

参考文档：[Server 架构设计](../apps/server-architecture-design.md)

## 项目现状

- ✅ `apps/web-app-backend` 已重命名为 `apps/server`
- ✅ 架构设计方案已完成
- ⚠️ `apps/server` 当前只有占位符代码
- ⚠️ `apps/web-app-frontend` 没有后端连接
- ⚠️ `apps/vscode-app` 没有专用服务端

## 阶段划分和时间估计

| 阶段 | 工作量 | 交付物 | 依赖 |
|-----|------|------|------|
| Phase 1 | 3-4 天 | Express Server 框架 + SDK 集成 | 无 |
| Phase 2 | 5-6 天 | 核心适配器 + REST API 基础端点 | Phase 1 |
| Phase 3 | 3-4 天 | WebSocket/SSE 实时通信 | Phase 2 |
| Phase 4 | 4-5 天 | Web 前端 API 集成 + API Client 库 | Phase 2 |
| Phase 5 | 3-4 天 | VSCode 前端 API 集成 | Phase 2 |
| Phase 6 | 2-3 天 | 测试、文档、性能优化 | Phase 3-5 |

**总工作量**: 20-26 天（单人开发）

---

## Phase 1: 基础设施和 SDK 集成

### 目标

建立 Server 的基础框架，完整初始化 SDK，建立依赖注入容器。

### 任务清单

#### 1.1 Server 应用框架
- [ ] 升级 `apps/server/package.json`
  - 添加 `express`、`typescript`、`dotenv` 依赖
  - 添加脚本：`dev`、`build`、`start`、`typecheck`
  - 设置 `type: "module"` 和 `main: "dist/index.js"`
- [ ] 创建 `apps/server/src/index.ts` 主入口
  - 基础日志输出
  - 环境变量加载
  - 待实现的占位符函数

#### 1.2 配置系统
- [ ] 复用 CLI-app 的配置加载逻辑
  - 复制 `apps/cli-app/src/config/` 到 `apps/server/src/config/`
  - 修改以适配 Server 环境（日志目录、端口配置等）
- [ ] 添加 Server 特定配置
  - 端口（默认 3000）
  - 主机（默认 0.0.0.0）
  - CORS 配置
  - WebSocket 配置

#### 1.3 日志系统
- [ ] 复用 CLI-app 的日志系统
  - 复制 `apps/cli-app/src/utils/logger.ts` 和 `output.ts`
  - 调整日志格式（移除 TUI 相关逻辑）

#### 1.4 存储管理器
- [ ] 复用 CLI-app 的存储管理器
  - 复制 `apps/cli-app/src/storage/index.ts` 到 `apps/server/src/storage/`
  - 无需修改（完全复用）

#### 1.5 SDK 初始化
- [ ] 创建 `apps/server/src/sdk-bootstrap.ts`
  - 加载配置
  - 初始化存储管理器
  - 创建 SDK 实例
  - 等待 SDK 就绪
  - 注册配置索引解析器（registerAllIndexResolvers）
  - 实现优雅关闭处理

#### 1.6 依赖注入容器
- [ ] 创建 `apps/server/src/services/container.ts`
  - 参考 CLI-app 的 `CLIDependencyContainer`
  - 实现 `ServerDependencyContainer` 类
  - 支持注册和获取适配器、服务、事件总线

#### 1.7 基础 Server 启动
- [ ] 创建 `apps/server/src/server.ts`
  - 创建 Express 应用
  - 配置基础中间件（日志、错误处理、CORS）
  - 实现 `start()` 和 `shutdown()` 方法
  - 配置优雅关闭（SIGTERM、SIGINT）

### 验收标准

- [ ] `pnpm --filter @wf-agent/server build` 成功编译
- [ ] `node apps/server/dist/index.js` 启动无错
- [ ] 日志输出：SDK 初始化完成
- [ ] `Ctrl+C` 能优雅关闭
- [ ] 没有类型错误

### 依赖项

- @wf-agent/sdk
- @wf-agent/config-processor
- @wf-agent/storage
- @wf-agent/types
- express

---

## Phase 2: 核心 API 层和适配器

### 目标

实现核心的 REST API 端点和适配器层，支持 Workflow、Tool、Template 等资源的查询和操作。

### 任务清单

#### 2.1 基础适配器框架
- [ ] 创建 `apps/server/src/adapters/base-adapter.ts`
  - 从 CLI-app 复制 `BaseAdapter` 基类
  - 无需修改

#### 2.2 核心适配器实现
- [ ] 创建 `apps/server/src/adapters/workflow-adapter.ts`
  - `list(query?: QueryOptions): Promise<Workflow[]>`
  - `get(id: string): Promise<Workflow>`
  - `create(data: WorkflowInput): Promise<Workflow>`
  - `update(id: string, data: Partial<WorkflowInput>): Promise<Workflow>`
  - `delete(id: string): Promise<void>`
  - `getGraph(id: string): Promise<WorkflowGraph>`
  
- [ ] 创建 `apps/server/src/adapters/tool-adapter.ts`
  - `list(query?: QueryOptions): Promise<Tool[]>`
  - `get(id: string): Promise<Tool>`
  - `validate(id: string, config: any): Promise<ValidationResult>`
  
- [ ] 创建 `apps/server/src/adapters/template-adapter.ts`
  - `list(query?: QueryOptions): Promise<Template[]>`
  - `get(id: string): Promise<Template>`
  - `instantiate(id: string, params: any): Promise<Workflow>`
  
- [ ] 创建 `apps/server/src/adapters/checkpoint-adapter.ts`
  - `list(executionId: string): Promise<Checkpoint[]>`
  - `get(id: string): Promise<Checkpoint>`
  - `restore(id: string): Promise<ExecutionId>`
  
- [ ] 创建其他必要的适配器（LLMProfile, Message, Event, Agent 等）
  - 参考 CLI-app 中的适配器清单

#### 2.3 执行服务
- [ ] 创建 `apps/server/src/services/execution-service.ts`
  - `execute(workflowId: string, input: any): Promise<ExecutionId>`
  - `getStatus(executionId: string): Promise<ExecutionStatus>`
  - `pause(executionId: string): Promise<void>`
  - `resume(executionId: string): Promise<void>`
  - `stop(executionId: string): Promise<void>`
  - `getLogs(executionId: string, pagination?: PaginationOptions): Promise<LogEntry[]>`
  - 内部事件发送（execution.started、execution.updated 等）

#### 2.4 REST API 路由
- [ ] 创建 `apps/server/src/routes/workflow.ts`
  ```
  GET    /workflows              - list
  POST   /workflows              - create
  GET    /workflows/:id          - get
  PUT    /workflows/:id          - update
  DELETE /workflows/:id          - delete
  GET    /workflows/:id/graph    - getGraph
  POST   /workflows/:id/execute  - execute
  ```
  
- [ ] 创建 `apps/server/src/routes/execution.ts`
  ```
  GET    /executions                      - list
  GET    /executions/:executionId         - get
  POST   /executions/:executionId/pause   - pause
  POST   /executions/:executionId/resume  - resume
  POST   /executions/:executionId/stop    - stop
  GET    /executions/:executionId/logs    - getLogs
  ```
  
- [ ] 创建 `apps/server/src/routes/tool.ts`
  ```
  GET    /tools                   - list
  GET    /tools/:id               - get
  POST   /tools/:id/validate      - validate
  ```
  
- [ ] 创建其他必要的路由（template、checkpoint 等）

#### 2.5 中间件和错误处理
- [ ] 创建 `apps/server/src/middleware/error-handler.ts`
  - 统一错误响应格式
  - HTTP 状态码映射
  - 日志记录
  
- [ ] 创建 `apps/server/src/middleware/request-logger.ts`
  - 记录所有请求（方法、路径、耗时、状态码）

#### 2.6 响应格式化
- [ ] 创建 `apps/server/src/utils/response.ts`
  - `successResponse(data, meta?)`
  - `errorResponse(error)`
  - 确保所有响应格式一致

#### 2.7 容器集成
- [ ] 在 `apps/server/src/index.ts` 中
  - 初始化 SDK 和容器
  - 注册所有适配器到容器
  - 注册执行服务等业务服务
  - 创建 Express 应用并配置路由

### 验收标准

- [ ] 所有核心路由都有实现
- [ ] 请求/响应格式符合设计规范
- [ ] 错误响应包括 code、message、details
- [ ] 所有适配器都通过容器注入
- [ ] 可以通过 curl 测试基础 API
- [ ] 没有类型错误
- [ ] 日志清晰记录每个请求

### 测试建议

```bash
# 启动 server
pnpm --filter @wf-agent/server dev

# 在另一个终端测试
curl http://localhost:3000/api/v1/workflows
curl http://localhost:3000/api/v1/tools
curl -X POST http://localhost:3000/api/v1/workflows/test-wf/execute -d '{}'
```

### 依赖项

- Phase 1 完成
- SDK 所有资源接口

---

## Phase 3: WebSocket/SSE 实时通信

### 目标

实现实时事件推送，支持 WebSocket 和 SSE 两种机制。

### 任务清单

#### 3.1 事件总线系统
- [ ] 创建 `apps/server/src/services/event-bus.ts`
  - 基于 RxJS 或 EventEmitter
  - `subscribe(topic: string, handler: Handler): Unsubscribe`
  - `publish(topic: string, event: Event): void`
  - 支持通配符订阅（`execution:*`）

#### 3.2 WebSocket 支持
- [ ] 安装 `ws` 库
- [ ] 创建 `apps/server/src/websocket/index.ts`
  - 创建 WebSocketServer
  - 管理客户端连接
  - 实现消息发送
  
- [ ] 创建 `apps/server/src/websocket/handlers.ts`
  - `subscribe` 消息处理
  - `unsubscribe` 消息处理
  - 参数验证

#### 3.3 SSE 支持
- [ ] 创建 `apps/server/src/routes/events-sse.ts`
  - 创建 `/events` SSE 端点
  - 解析查询参数（resource、id 等）
  - 流式发送事件

#### 3.4 事件发布集成
- [ ] 修改 `ExecutionService`
  - 在关键点发布事件
  - `execution:started` - 执行开始
  - `execution:updated` - 执行更新（进度、日志）
  - `execution:completed` - 执行完成
  - `execution:error` - 执行错误
  - `execution:paused` - 执行暂停
  
- [ ] 修改 SDK 集成
  - 监听 SDK 事件
  - 转发到事件总线

#### 3.5 连接生命周期管理
- [ ] WebSocket 连接管理
  - 心跳检测（ping/pong）
  - 超时自动断开
  - 重连处理
  
- [ ] SSE 连接管理
  - 客户端断开检测
  - 资源清理

#### 3.6 测试页面（可选）
- [ ] 创建 `apps/server/public/test-ws.html`
  - 用于测试 WebSocket 连接
  - 显示实时事件

### 验收标准

- [ ] 可以通过 WebSocket 连接 `/ws`
- [ ] 可以订阅执行事件并接收更新
- [ ] SSE 端点工作正常
- [ ] 事件格式一致
- [ ] 连接断开后自动清理资源
- [ ] 心跳检测防止连接超时

### 测试建议

```javascript
// WebSocket 测试
const ws = new WebSocket('ws://localhost:3000/ws');
ws.onopen = () => {
  ws.send(JSON.stringify({
    type: 'subscribe',
    resource: 'execution',
    id: 'execution-123'
  }));
};
ws.onmessage = (e) => console.log(JSON.parse(e.data));
```

### 依赖项

- Phase 2 完成
- ws 库
- RxJS 或 Node.js EventEmitter

---

## Phase 4: Web 前端 API 集成

### 目标

将 Web 前端与 Server 连接，实现完整的前后端交互。

### 任务清单

#### 4.1 API Client 库（可选但推荐）
- [ ] 创建 `packages/server-api-client/` 包
  - 统一的 API 请求封装
  - 自动错误处理和重试
  - 类型安全的 API 调用
  
- [ ] 实现 `APIClient` 基类
  - 基 URL 配置
  - 请求拦截器（认证、日志）
  - 响应拦截器（错误处理）
  
- [ ] 实现各资源 API 类
  - `WorkflowAPI`、`ToolAPI`、`TemplateAPI` 等
  - 自动生成的类型安全方法

#### 4.2 Web 前端配置
- [ ] 更新 `apps/web-app-frontend/package.json`
  - 如果用了 API Client 库，添加依赖
  
- [ ] 创建 `apps/web-app-frontend/src/lib/api/client.ts`
  - 初始化 APIClient 实例
  - 配置 API 基础 URL（env 变量）

#### 4.3 Svelte Stores 改造
- [ ] 创建 `apps/web-app-frontend/src/stores/workflows.ts`
  ```typescript
  export const workflows = readable([], (set) => {
    apiClient.workflows.list().then(set);
  });
  
  export const currentWorkflow = writable<Workflow | null>(null);
  ```
  
- [ ] 创建 `apps/web-app-frontend/src/stores/executions.ts`
  - 执行列表
  - 当前执行详情
  - 实时事件订阅

#### 4.4 页面集成
- [ ] 修改 `src/routes/workflows/+page.svelte`
  - 使用 workflows store 显示列表
  - 实现创建、编辑、删除操作
  
- [ ] 修改 `src/routes/executions/+page.svelte`
  - 使用 executions store
  - 显示执行列表和实时状态
  - 实现暂停、恢复、停止操作
  
- [ ] 修改相关组件
  - 工作流详情组件
  - 执行监控组件
  - 日志查看组件

#### 4.5 环境配置
- [ ] 创建 `.env.local`（仅开发）
  ```
  VITE_API_BASE_URL=http://localhost:3000/api/v1
  ```
  
- [ ] 更新 `.env.production`
  ```
  VITE_API_BASE_URL=/api/v1
  ```

#### 4.6 集成测试
- [ ] 在本地同时运行 Server 和 Web 前端
  - Server: `pnpm --filter @wf-agent/server dev`
  - Web: `pnpm --filter @wf-agent/web-app-frontend dev`
  - 测试各个功能

### 验收标准

- [ ] 工作流列表能正确加载并显示
- [ ] 可以执行工作流并看到实时进度
- [ ] 暂停、恢复、停止功能工作正常
- [ ] 错误处理友好（显示错误消息）
- [ ] 没有控制台错误或警告

### 依赖项

- Phase 2 完成
- Phase 3 完成（可选，如果用实时事件）

---

## Phase 5: VSCode 前端 API 集成

### 目标

将 VSCode 扩展 WebView 与 Server 连接。

### 任务清单

#### 5.1 VSCode 扩展后端
- [ ] 更新 `apps/vscode-app/package.json`
  - 如果用了 API Client 库，添加依赖
  
- [ ] 创建 `apps/vscode-app/src/extension.ts`（如果不存在）
  - 启动 Server（可能是嵌入式或外部服务）
  - 或连接到外部 Server

#### 5.2 WebView 前端
- [ ] 创建 `apps/vscode-app/src/webview/` 目录
  ```
  src/webview/
  ├── App.svelte          # 主应用
  ├── api/
  │   └── client.ts       # API Client
  ├── stores/
  │   ├── workflows.ts
  │   └── executions.ts
  └── components/
      ├── WorkflowList.svelte
      └── ExecutionMonitor.svelte
  ```
  
- [ ] 创建 `apps/vscode-app/src/webview/api/client.ts`
  - 通过 vscode.postMessage 与后端通信
  - 或直接连接本地 Server（如果 Server 启动了）
  
- [ ] 创建 WebView Stores
  - 参考 Web 前端的 stores
  - 适配 VSCode 环境

#### 5.3 扩展和 WebView 通信
- [ ] 实现 `apps/vscode-app/src/extension.ts`
  - 创建 WebView panel
  - 处理 WebView 消息
  - 转发到 Server（如果 Server 是后端）
  
- [ ] 实现双向消息通道
  - WebView -> Extension -> Server -> WebView

#### 5.4 UI/UX 调整
- [ ] 根据 VSCode 设计系统调整 UI
  - 使用 VSCode 主题色
  - 使用 VSCode Web Components

#### 5.5 集成测试
- [ ] 在 VSCode 中加载扩展
- [ ] 测试 WebView 功能
- [ ] 验证与 Server 通信

### 验收标准

- [ ] WebView 可以加载并显示内容
- [ ] 可以从 WebView 调用 Server API
- [ ] 工作流列表、执行等功能工作正常
- [ ] 错误处理正确

### 依赖项

- Phase 2 完成
- Phase 3 完成（可选）

---

## Phase 6: 测试、文档和优化

### 目标

确保系统质量，完善文档，优化性能。

### 任务清单

#### 6.1 单元测试
- [ ] 编写适配器单元测试
  - `apps/server/src/adapters/__tests__/`
  - 至少 80% 覆盖率
  
- [ ] 编写服务单元测试
  - `apps/server/src/services/__tests__/`
  
- [ ] 编写中间件和工具函数测试

#### 6.2 集成测试
- [ ] 创建 `apps/server/__tests__/integration/`
  - API 端点集成测试
  - 工作流执行完整流程测试
  - WebSocket 事件测试

#### 6.3 端到端测试
- [ ] Web 前端 E2E 测试（如使用 Playwright）
  - 工作流管理功能
  - 执行监控功能
  
- [ ] VSCode 扩展 E2E 测试

#### 6.4 文档完善
- [ ] 创建 `docs/apps/server/` 目录
  - `setup.md` - 开发环境设置
  - `api-reference.md` - API 文档
  - `architecture.md` - 架构说明（从设计文档提取）
  - `deployment.md` - 部署指南
  
- [ ] 更新 README
  - 项目结构说明
  - 快速开始
  - 开发命令

#### 6.5 性能优化
- [ ] 添加请求缓存
  - 工作流列表缓存
  - 工具列表缓存
  
- [ ] 数据库连接池配置（如适用）
  
- [ ] API 响应分页
  - 列表 API 支持 offset/limit
  
- [ ] 日志等级调整
  - 生产环境使用较低等级

#### 6.6 安全加固
- [ ] CORS 配置
  - 严格限制允许的来源
  
- [ ] 输入验证
  - 所有 API 输入都经过验证
  
- [ ] 错误信息脱敏
  - 生产环境不暴露技术细节

#### 6.7 部署配置
- [ ] Dockerfile 创建
  - Server 容器镜像
  
- [ ] docker-compose.yml
  - 本地开发环境配置
  
- [ ] 环境变量文档
  - .env.example 文件

#### 6.8 监控和日志
- [ ] 添加性能指标
  - 请求耗时分布
  - 错误率
  
- [ ] 添加应用监控钩子（可选）

### 验收标准

- [ ] 所有单元测试通过
- [ ] 集成测试覆盖核心功能
- [ ] API 文档完整
- [ ] 可以通过 Docker 启动
- [ ] 生产环境配置完成

### 依赖项

- Phase 2-5 完成

---

## 跨阶段的通用任务

### 代码质量

在每个阶段都应该进行：
- 类型检查通过：`pnpm typecheck`
- Linting 通过：`pnpm lint`
- 代码格式化：`pnpm format`

### 版本控制

- 每个阶段完成后提交 PR
- 主要功能点有对应的 commit 信息
- 参考格式：`feat(server): implement workflow adapter` 或 `docs(server): add API reference`

### 沟通和同步

- 定期检查架构设计是否需要调整
- 根据实际开发情况灵活调整计划
- 记录任何架构决策变更

---

## 依赖关系图

```
Phase 1
  │
  ├─→ Phase 2 (REST API + Adapters)
  │     │
  │     ├─→ Phase 3 (WebSocket/SSE)
  │     │     │
  │     │     └─→ Phase 6 (Testing)
  │     │
  │     ├─→ Phase 4 (Web Integration)
  │     │     │
  │     │     └─→ Phase 6
  │     │
  │     └─→ Phase 5 (VSCode Integration)
  │           │
  │           └─→ Phase 6
```

**关键路径**：Phase 1 → Phase 2 → (Phase 3 或 Phase 4/5) → Phase 6

---

## 风险和缓解策略

| 风险 | 影响 | 缓解策略 |
|-----|------|---------|
| SDK 集成复杂性高 | Phase 1 延期 | 提前详细阅读 SDK 源码 |
| WebSocket 连接稳定性 | Phase 3 困难 | 充分测试，考虑降级到 SSE |
| 前端框架差异（Web vs VSCode） | Phase 4/5 延期 | 抽象共用的 API Client 层 |
| 测试覆盖率难达标 | Phase 6 延期 | 优先覆盖关键路径 |

---

## 验收清单

### Phase 1 完成
- [ ] SDK 成功初始化并就绪
- [ ] Server 框架可启动
- [ ] 依赖容器工作正常
- [ ] 优雅关闭功能验证

### Phase 2 完成
- [ ] 所有核心适配器实现
- [ ] REST API 端点完整
- [ ] 可以通过 curl 测试所有端点
- [ ] 错误处理和日志完善

### Phase 3 完成
- [ ] WebSocket 服务工作正常
- [ ] SSE 端点可用
- [ ] 实时事件推送验证
- [ ] 连接生命周期管理正确

### Phase 4 完成
- [ ] Web 前端能加载并显示数据
- [ ] 前后端通信正常
- [ ] 工作流执行完整流程工作
- [ ] 实时状态更新显示正确

### Phase 5 完成
- [ ] VSCode 扩展 WebView 加载
- [ ] WebView API 通信正常
- [ ] 功能与 Web 前端一致
- [ ] VSCode 环境特定问题解决

### Phase 6 完成
- [ ] 测试覆盖率达标
- [ ] 文档完整
- [ ] 性能指标满足
- [ ] 部署配置就绪

---

## 后续维护和迭代

### 短期（1-2 月）
- 用户反馈收集和 bug 修复
- 性能瓶颈优化
- 前端 UX 改进

### 中期（2-3 月）
- 认证和授权系统
- 多用户支持
- 高可用部署配置

### 长期（3-6 月）
- 移动端支持
- 离线模式
- 插件系统扩展
