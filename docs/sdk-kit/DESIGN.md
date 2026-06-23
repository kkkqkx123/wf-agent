# SDK-Kit 设计文档

**版本**: 1.0.0  
**状态**: 设计阶段  
**最后更新**: 2026/06/22

---

## 1. 概述

SDK-Kit 是 Workflow SDK 的高级包装器，提供简洁流畅的 API 接口。目标是降低 SDK 使用复杂度，而不是添加新功能。

**核心原则**:
- 简洁优先：最少化功能，最大化易用性
- 最终形态：代码直接可用于生产，无临时状态
- 单一职责：仅作为 SDK 适配层，不处理缓存/指标/验证
- 类型安全：充分利用 TypeScript 类型系统

---

## 2. API 设计

### 2.1 核心 API 结构

```typescript
class SDKKit {
  // 初始化
  constructor(sdk: SDK)

  // 四个主要 API
  workflow(): WorkflowAPI      // 工作流定义
  execution(): ExecutionAPI    // 工作流执行  
  query(): QueryAPI            // 执行记录查询
  resource(): ResourceAPI      // 工作流资源管理
  
  // 工具方法
  getSDK(): SDK                // 访问底层 SDK
}
```

### 2.2 WorkflowAPI - 流畅式工作流定义

**设计目标**: 使用链式 API 优雅地定义工作流

```typescript
interface WorkflowAPI {
  create(id: string): WorkflowBuilder;
}

interface WorkflowBuilder {
  node(id: string, config: NodeConfig): WorkflowBuilder;
  edge(source: string, target: string, config?: EdgeConfig): WorkflowBuilder;
  build(): WorkflowTemplate;
}
```

**使用示例**:
```typescript
const template = kit.workflow()
  .create('my-workflow')
  .node('start', { type: 'START' })
  .node('task', { type: 'LLM', config: { model: 'claude-3' } })
  .node('end', { type: 'END' })
  .edge('start', 'task')
  .edge('task', 'end')
  .build();
```

**当前实现状态**: ✅ 完成

### 2.3 ExecutionAPI - 工作流执行

**设计目标**: 以最少的参数执行工作流，提供直观的配置选项

```typescript
interface ExecutionAPI {
  workflow(id: string): ExecutionBuilder;
}

interface ExecutionBuilder {
  input(data: Record<string, unknown>): ExecutionBuilder;
  timeout(ms: number): ExecutionBuilder;
  execute(): Promise<ExecutionResult>;
}

interface ExecutionResult {
  id: string;
  status: 'completed' | 'failed' | 'timeout';
  output?: unknown;
  error?: Error;
  duration: number;
}
```

**使用示例**:
```typescript
const result = await kit.execution()
  .workflow('my-workflow')
  .input({ data: 'test' })
  .timeout(30000)
  .execute();
```

**当前实现状态**: ✅ 完成

### 2.4 QueryAPI - 执行记录查询

**设计目标**: 灵活的查询、过滤、排序和分页

```typescript
interface QueryAPI {
  executions(): QueryBuilder;
}

interface QueryBuilder {
  filter(criteria: FilterCriteria): QueryBuilder;
  sort(options: SortOptions): QueryBuilder;
  limit(n: number): QueryBuilder;
  offset(n: number): QueryBuilder;
  get(): Promise<ExecutionRecord[]>;
}

interface FilterCriteria {
  workflowId?: string;
  status?: 'completed' | 'failed' | 'running';
  startTime?: number;
  endTime?: number;
  [key: string]: unknown;
}

interface SortOptions {
  [field: string]: 'asc' | 'desc';
}
```

**使用示例**:
```typescript
const records = await kit.query()
  .executions()
  .filter({ status: 'completed', workflowId: 'my-wf' })
  .sort({ createdAt: 'desc' })
  .limit(10)
  .offset(0)
  .get();
```

**当前实现状态**: ✅ 完成

### 2.5 ResourceAPI - 工作流资源管理

**设计目标**: CRUD 操作和基本的版本管理

```typescript
interface ResourceAPI {
  workflows(): WorkflowResource;
}

interface WorkflowResource {
  create(template: WorkflowTemplate): Promise<string>;
  read(id: string): Promise<WorkflowTemplate>;
  update(id: string, updates: Partial<WorkflowTemplate>): Promise<void>;
  delete(id: string): Promise<void>;
  list(filter?: ResourceFilter): Promise<WorkflowTemplate[]>;
  exists(id: string): Promise<boolean>;
  
  // 版本管理（可选，如果 SDK 支持）
  getVersion(id: string): Promise<string>;
  listVersions(id: string): Promise<WorkflowVersion[]>;
  rollback(id: string, version: string): Promise<void>;
}

interface ResourceFilter {
  tag?: string;
  status?: string;
  [key: string]: unknown;
}
```

**使用示例**:
```typescript
// 创建
const id = await kit.resource()
  .workflows()
  .create(template);

// 读取
const wf = await kit.resource()
  .workflows()
  .read(id);

// 更新
await kit.resource()
  .workflows()
  .update(id, { description: 'New description' });

// 删除
await kit.resource()
  .workflows()
  .delete(id);

// 列表
const workflows = await kit.resource()
  .workflows()
  .list({ tag: 'production' });
```

**当前实现状态**: ✅ 完成

---

## 3. 错误处理

**设计原则**: 统一的错误类型，清晰的错误信息

```typescript
class KitError extends Error {
  code: KitErrorCode;
  context?: Record<string, unknown>;
}

enum KitErrorCode {
  VALIDATION_ERROR = 'VALIDATION_ERROR',
  NOT_FOUND = 'NOT_FOUND',
  INTERNAL_ERROR = 'INTERNAL_ERROR',
  EXECUTION_ERROR = 'EXECUTION_ERROR',
}
```

**错误转换**:
- SDK 返回的错误 → KitError
- 验证失败 → VALIDATION_ERROR
- 资源不存在 → NOT_FOUND
- SDK 内部错误 → INTERNAL_ERROR
- 执行失败 → EXECUTION_ERROR

**使用示例**:
```typescript
try {
  await kit.execution().workflow('wf-1').execute();
} catch (error) {
  if (error instanceof KitError) {
    switch (error.code) {
      case KitErrorCode.NOT_FOUND:
        console.log('Workflow not found');
        break;
      case KitErrorCode.VALIDATION_ERROR:
        console.log('Invalid input:', error.context);
        break;
      default:
        console.error('Error:', error.message);
    }
  }
}
```

---

## 4. 不实现的功能

以下功能**不在 SDK-Kit 范围内**，由上层应用处理：

| 功能 | 原因 |
|------|------|
| **缓存** | 应用层更清楚缓存策略，SDK-Kit 不应引入数据一致性问题 |
| **指标收集** | 应用有不同的监控需求，应使用专门的监控库 |
| **批处理 API** | 简单使用 Promise.all，无需封装 |
| **验证框架** | TypeScript 类型系统已足够，运行时验证由应用决定 |
| **事件系统** | 应用可直接订阅 SDK 的事件 |

**批处理示例** (由应用处理):
```typescript
// 批量创建工作流
const ids = await Promise.all(
  templates.map(t => kit.resource().workflows().create(t))
);

// 批量删除工作流
await Promise.all(
  ids.map(id => kit.resource().workflows().delete(id))
);
```

---

## 5. 类型定义

### 公共类型

```typescript
// 工作流定义
interface WorkflowTemplate {
  id: string;
  version?: string;
  description?: string;
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
  metadata?: Record<string, unknown>;
}

interface WorkflowNode {
  id: string;
  type: string;
  config?: Record<string, unknown>;
}

interface WorkflowEdge {
  source: string;
  target: string;
  config?: Record<string, unknown>;
}

// 执行记录
interface ExecutionRecord {
  id: string;
  workflowId: string;
  status: 'completed' | 'failed' | 'running';
  input?: unknown;
  output?: unknown;
  error?: Error;
  startTime: number;
  endTime?: number;
  duration?: number;
}

// 版本管理（可选）
interface WorkflowVersion {
  version: string;
  createdAt: number;
  createdBy?: string;
}
```

### SDK 接口

```typescript
interface SDK {
  version?: string;
  executeCommand(command: unknown): Promise<Result<ExecutionResult>>;
  getFactory(): SDKFactory;
  ExecuteWorkflowCommand?: unknown;
  [key: string]: unknown;
}

interface SDKFactory {
  getDependencies(): Record<string, unknown>;
  getWorkflowRegistry(): WorkflowRegistry;
  getWorkflowExecutionRegistry(): WorkflowExecutionRegistry;
  getEventEmitter(): EventEmitter;
}

interface WorkflowRegistry {
  create(template: WorkflowTemplate): Promise<Result<string>>;
  get(id: string): Promise<Result<WorkflowTemplate>>;
  update(id: string, updates: Partial<WorkflowTemplate>): Promise<Result<void>>;
  delete(id: string): Promise<Result<void>>;
  list(filter?: Record<string, unknown>): Promise<Result<WorkflowTemplate[]>>;
  [key: string]: unknown;
}

interface WorkflowExecutionRegistry {
  query(options: unknown): Promise<Result<ExecutionRecord[]>>;
}

interface Result<T, E = Error> {
  success: boolean;
  data?: T;
  error?: E;
}
```

---

## 6. 文件结构

```
packages/sdk-kit/
├── src/
│   ├── index.ts                    # 公开导出
│   ├── kit.ts                      # 主类
│   ├── api/
│   │   ├── workflow.api.ts         # WorkflowAPI
│   │   ├── execution.api.ts        # ExecutionAPI
│   │   ├── query.api.ts            # QueryAPI
│   │   ├── resource.api.ts         # ResourceAPI
│   │   └── index.ts
│   ├── executors/
│   │   ├── execution.executor.ts   # ExecutionAPI 实现
│   │   ├── query.executor.ts       # QueryAPI 实现
│   │   └── index.ts
│   ├── managers/
│   │   ├── resource.manager.ts     # ResourceAPI 实现
│   │   ├── event.manager.ts        # 事件管理（如果需要）
│   │   └── index.ts
│   ├── converters/
│   │   ├── error.converter.ts      # SDK 错误转换
│   │   └── index.ts
│   ├── builders/
│   │   ├── workflow.builder.ts     # WorkflowBuilder 实现
│   │   └── index.ts
│   └── types/
│       ├── index.ts
│       ├── common.types.ts
│       ├── workflow.types.ts
│       ├── execution.types.ts
│       ├── query.types.ts
│       ├── resource.types.ts
│       └── sdk.types.ts
├── __tests__/
│   ├── unit/
│   │   ├── builders/
│   │   ├── apis/
│   │   └── converters/
│   └── integration/
│       └── full-workflow.test.ts
├── package.json
├── tsconfig.json
└── vitest.config.mjs
```

---

## 7. 实现清单

- [x] WorkflowAPI - 工作流定义
- [x] ExecutionAPI - 工作流执行
- [x] QueryAPI - 执行记录查询
- [x] ResourceAPI - 工作流 CRUD
- [x] 错误处理和转换
- [x] 类型定义
- [ ] 单元测试覆盖
- [ ] 集成测试覆盖
- [ ] 文档和示例

---

## 8. 使用示例

### 完整工作流

```typescript
import { SDKKit } from '@wf-agent/sdk-kit';
import { createSDK } from '@wf-agent/sdk';

// 初始化
const sdk = createSDK();
const kit = new SDKKit(sdk);

// 1. 定义工作流
const template = kit.workflow()
  .create('data-pipeline')
  .node('start', { type: 'START' })
  .node('fetch', { type: 'TOOL_CALL', config: { tool: 'fetch' } })
  .node('process', { type: 'LLM' })
  .node('save', { type: 'TOOL_CALL', config: { tool: 'save' } })
  .node('end', { type: 'END' })
  .edge('start', 'fetch')
  .edge('fetch', 'process')
  .edge('process', 'save')
  .edge('save', 'end')
  .build();

// 2. 创建工作流资源
const workflowId = await kit.resource()
  .workflows()
  .create(template);

// 3. 执行工作流
const result = await kit.execution()
  .workflow(workflowId)
  .input({ url: 'https://api.example.com/data' })
  .timeout(30000)
  .execute();

if (result.status === 'completed') {
  console.log('Success:', result.output);
} else {
  console.error('Failed:', result.error);
}

// 4. 查询执行记录
const records = await kit.query()
  .executions()
  .filter({ workflowId, status: 'completed' })
  .sort({ startTime: 'desc' })
  .limit(10)
  .get();

// 5. 更新工作流
await kit.resource()
  .workflows()
  .update(workflowId, { description: 'Updated' });

// 6. 删除工作流
await kit.resource()
  .workflows()
  .delete(workflowId);
```

---

## 9. 设计决策

### 为什么不实现缓存？

- ❌ 数据一致性复杂：资源可能被外部修改
- ❌ 缓存策略多样：不同应用有不同需求
- ❌ 调试困难：缓存命中/未命中难以追踪
- ✅ 应用层实现更灵活：按需缓存，可自定义

### 为什么不实现验证？

- ❌ TypeScript 类型系统已提供编译时检查
- ❌ 运行时验证应该由应用决定
- ❌ 过度设计会增加维护成本
- ✅ 简洁优先原则

### 为什么是流畅 API？

- ✅ 代码可读性高
- ✅ IDE 类型提示友好
- ✅ 易于理解和学习
- ✅ 减少参数传递

### 为什么分离 Managers 和 APIs？

- ✅ Managers 处理业务逻辑
- ✅ APIs 提供公开接口
- ✅ 解耦关注点
- ✅ 便于单元测试

---

## 10. 验收标准

**功能完整性**:
- ✓ 所有四个主要 API 可用
- ✓ 错误正确转换和处理
- ✓ 类型定义完整准确

**代码质量**:
- ✓ 无 TypeScript 错误或警告
- ✓ 所有函数有 JSDoc 注释
- ✓ 代码风格统一

**生产就绪**:
- ✓ 无临时代码或注释
- ✓ 错误处理完善
- ✓ 安全的类型转换

