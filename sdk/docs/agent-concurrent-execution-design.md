# Agent 并发执行场景与架构设计重新分析

## 概述

本文档重新分析 Agent Loop 的并发执行场景，评估是否需要 Pool/Queue 架构，并提出统一的设计方案。

---

## 1. Agent 并发执行场景分析

### 1.1 Sub-Agent 与主 Agent 并发

**场景描述**:
- 主 Agent 执行过程中调用 Sub-Agent
- Sub-Agent 独立执行，可能需要并发
- 主 Agent 等待 Sub-Agent 结果

**示例**:
```
主 Agent (代码审查)
  ├─ Sub-Agent 1 (审查模块 A)
  ├─ Sub-Agent 2 (审查模块 B)
  └─ Sub-Agent 3 (审查模块 C)
     ↓
  汇总结果
```

**并发需求**:
- 多个 Sub-Agent 可以并行执行
- 需要限制并发数量（避免 API 限流）
- 需要管理 Sub-Agent 的生命周期

### 1.2 多 Agent 在不同 Git Worktree 并发

**场景描述**:
- 多个 Agent 在不同的 git worktree 中工作
- 每个 Agent 独立修改不同的分支
- 需要并发执行以提高效率

**示例**:
```
项目根目录
  ├─ worktree/feature-a (Agent 1 修改 feature-a 分支)
  ├─ worktree/feature-b (Agent 2 修改 feature-b 分支)
  └─ worktree/feature-c (Agent 3 修改 feature-c 分支)
```

**并发需求**:
- 多个 Agent 可以并行执行
- 每个 Agent 独立的工作目录
- 需要资源隔离和管理

### 1.3 其他并发场景

**批量任务处理**:
- 批量处理多个文件
- 批量执行多个测试
- 批量生成多个报告

**多用户场景**:
- 多个用户同时使用 Agent
- 每个 Agent 独立的会话
- 需要限制总并发数

---

## 2. Agent 并发与 Graph 并发的对比

### 2.1 并发性质对比

| 特性 | Graph 并发 | Agent 并发 |
|------|-----------|-----------|
| 触发方式 | 触发器触发子工作流 | 主 Agent 调用 Sub-Agent |
| 并发单元 | ThreadEntity | AgentLoopEntity |
| 依赖关系 | 子工作流可能依赖父工作流 | Sub-Agent 可能依赖主 Agent |
| 资源隔离 | 共享 Graph Registry | 可能需要工作目录隔离 |
| 结果处理 | 回调或 Promise | Promise 或 Stream |

### 2.2 相似之处

1. **并发执行**: 都需要并发执行多个实例
2. **资源管理**: 都需要限制并发数量
3. **任务调度**: 都需要队列管理
4. **执行器复用**: 都可以复用执行器

### 2.3 差异之处

| 特性 | Graph | Agent |
|------|-------|-------|
| 执行器 | ThreadExecutor | AgentLoopExecutor |
| 实例管理 | ThreadRegistry | AgentLoopRegistry |
| 任务注册 | TaskRegistry | 可复用 TaskRegistry |
| 事件系统 | EventManager | EventManager |
| 工作目录 | 无需隔离 | 可能需要隔离 |

---

## 3. 统一执行管理架构设计

### 3.1 设计原则

1. **统一抽象**: Graph 和 Agent 共享执行管理基础设施
2. **差异化实现**: 保留各自的执行器实现
3. **资源复用**: 复用 Pool/Queue/Registry 基础设施
4. **灵活配置**: 支持不同的并发策略

### 3.2 统一架构

```
统一执行管理架构:
┌─────────────────────────────────────────────────────────┐
│           ExecutionManager (统一接口)                    │
│  - execute(instance): Promise<Result>                   │
│  - executeSync(instance): Promise<Result>               │
│  - executeAsync(instance): TaskSubmissionResult         │
│  - cancel(id): Promise<boolean>                         │
│  - getStatus(id): TaskInfo | null                       │
└─────────────────────┬───────────────────────────────────┘
                      │
        ┌─────────────┴─────────────┐
        │                           │
        ▼                           ▼
┌───────────────────┐     ┌───────────────────┐
│ ThreadExecution   │     │ AgentExecution    │
│ Manager           │     │ Manager           │
│ (Graph 专用)      │     │ (Agent 专用)      │
└─────────┬─────────┘     └─────────┬─────────┘
          │                         │
          └─────────────┬───────────┘
                        │
        ┌───────────────┼───────────────┐
        │               │               │
        ▼               ▼               ▼
┌───────────┐   ┌───────────┐   ┌───────────┐
│TaskRegistry│   │ExecutionPool│   │ExecutionQueue│
│(共享)      │   │Service     │   │Manager      │
│            │   │(共享)      │   │(共享)       │
└───────────┘   └───────────┘   └───────────┘
```

### 3.3 共享基础设施

#### 3.3.1 TaskRegistry（已存在）

**位置**: `sdk/graph/services/task-registry.ts`

**已支持**:
- 统一的 TaskInfo 接口
- 支持 Agent 和 Thread 实例
- 任务状态管理

**无需修改**，可以直接复用。

#### 3.3.2 ExecutionPoolService（重命名和泛化）

**当前**: `ThreadPoolService`

**建议重命名**: `ExecutionPoolService`

**泛化设计**:
```typescript
// sdk/core/services/execution-pool-service.ts
export interface ExecutorFactory<T extends ExecutionInstance> {
  create(): Executor<T>;
}

export interface Executor<T extends ExecutionInstance> {
  execute(instance: T): Promise<Result>;
  cleanup(): void;
}

export class ExecutionPoolService<T extends ExecutionInstance> {
  constructor(
    private executorFactory: ExecutorFactory<T>,
    private config: ThreadPoolConfig
  ) {}

  async allocateExecutor(): Promise<Executor<T>>;
  async releaseExecutor(executor: Executor<T>): Promise<void>;
  getStats(): PoolStats;
  async shutdown(): Promise<void>;
}
```

#### 3.3.3 ExecutionQueueManager（重命名和泛化）

**当前**: `TaskQueueManager`

**建议重命名**: `ExecutionQueueManager`

**泛化设计**:
```typescript
// sdk/core/managers/execution-queue-manager.ts
export class ExecutionQueueManager<T extends ExecutionInstance> {
  constructor(
    private taskRegistry: TaskRegistry,
    private poolService: ExecutionPoolService<T>,
    private eventManager: EventManager
  ) {}

  async submitSync(
    instance: T,
    instanceType: ExecutionInstanceType,
    timeout?: number
  ): Promise<ExecutionResult>;

  submitAsync(
    instance: T,
    instanceType: ExecutionInstanceType,
    timeout?: number
  ): TaskSubmissionResult;

  cancelTask(taskId: string): boolean;
  getQueueStats(): QueueStats;
  async drain(): Promise<void>;
  clear(): void;
}
```

### 3.4 专用管理器

#### 3.4.1 ThreadExecutionManager

**位置**: `sdk/graph/managers/thread-execution-manager.ts`

```typescript
export class ThreadExecutionManager {
  private taskRegistry: TaskRegistry;
  private poolService: ExecutionPoolService<ThreadEntity>;
  private queueManager: ExecutionQueueManager<ThreadEntity>;

  constructor(deps: ThreadExecutionManagerDependencies) {
    this.taskRegistry = TaskRegistry.getInstance();
    this.poolService = new ExecutionPoolService(
      deps.executorFactory,
      deps.poolConfig
    );
    this.queueManager = new ExecutionQueueManager(
      this.taskRegistry,
      this.poolService,
      deps.eventManager
    );
  }

  async executeSync(threadEntity: ThreadEntity): Promise<ThreadResult> {
    return this.queueManager.submitSync(threadEntity, "thread");
  }

  executeAsync(threadEntity: ThreadEntity): TaskSubmissionResult {
    return this.queueManager.submitAsync(threadEntity, "thread");
  }

  // ... 其他方法
}
```

#### 3.4.2 AgentExecutionManager

**位置**: `sdk/agent/managers/agent-execution-manager.ts`

```typescript
export class AgentExecutionManager {
  private taskRegistry: TaskRegistry;
  private poolService: ExecutionPoolService<AgentLoopEntity>;
  private queueManager: ExecutionQueueManager<AgentLoopEntity>;

  constructor(deps: AgentExecutionManagerDependencies) {
    this.taskRegistry = TaskRegistry.getInstance();
    this.poolService = new ExecutionPoolService(
      deps.executorFactory,
      deps.poolConfig
    );
    this.queueManager = new ExecutionQueueManager(
      this.taskRegistry,
      this.poolService,
      deps.eventManager
    );
  }

  async executeSync(agentEntity: AgentLoopEntity): Promise<AgentLoopResult> {
    return this.queueManager.submitSync(agentEntity, "agent");
  }

  executeAsync(agentEntity: AgentLoopEntity): TaskSubmissionResult {
    return this.queueManager.submitAsync(agentEntity, "agent");
  }

  // ... 其他方法
}
```

---

## 4. 迁移方案

### 4.1 第一阶段：泛化基础设施

1. **重命名和泛化 ThreadPoolService**:
   - 重命名为 `ExecutionPoolService`
   - 泛化为支持 `ExecutionInstance`
   - 保持向后兼容（保留 `ThreadPoolService` 别名）

2. **重命名和泛化 TaskQueueManager**:
   - 重命名为 `ExecutionQueueManager`
   - 泛化为支持 `ExecutionInstance`
   - 保持向后兼容（保留 `TaskQueueManager` 别名）

3. **移动到 core 模块**:
   - `sdk/core/services/execution-pool-service.ts`
   - `sdk/core/managers/execution-queue-manager.ts`

### 4.2 第二阶段：创建 Agent 执行管理器

1. **创建 AgentExecutionManager**:
   - `sdk/agent/managers/agent-execution-manager.ts`
   - 使用共享的 Pool/Queue 基础设施

2. **创建 AgentExecutorFactory**:
   - `sdk/agent/factories/agent-executor-factory.ts`
   - 创建 AgentLoopExecutor 实例

3. **集成到 DI 容器**:
   - 注册 AgentExecutionManager
   - 配置 Agent Pool 参数

### 4.3 第三阶段：重构 Graph 执行管理

1. **创建 ThreadExecutionManager**:
   - `sdk/graph/managers/thread-execution-manager.ts`
   - 使用共享的 Pool/Queue 基础设施

2. **重构 TriggeredSubworkflowManager**:
   - 使用 ThreadExecutionManager
   - 简化内部实现

3. **保持向后兼容**:
   - 保留现有 API
   - 内部使用新架构

---

## 5. 配置示例

### 5.1 Graph Pool 配置

```typescript
const graphPoolConfig: ThreadPoolConfig = {
  minExecutors: 1,
  maxExecutors: 10,
  idleTimeout: 30000,
  defaultTimeout: 60000,
};
```

### 5.2 Agent Pool 配置

```typescript
const agentPoolConfig: ThreadPoolConfig = {
  minExecutors: 1,
  maxExecutors: 5,  // 限制并发 Agent 数量
  idleTimeout: 60000,
  defaultTimeout: 120000,
};
```

### 5.3 Sub-Agent 并发示例

```typescript
// 主 Agent 调用多个 Sub-Agent
const agentExecutionManager = getContainer().get(AgentExecutionManager);

// 并发执行多个 Sub-Agent
const results = await Promise.all([
  agentExecutionManager.executeSync(subAgent1),
  agentExecutionManager.executeSync(subAgent2),
  agentExecutionManager.executeSync(subAgent3),
]);

// 或者使用异步提交
const taskIds = [
  agentExecutionManager.executeAsync(subAgent1),
  agentExecutionManager.executeAsync(subAgent2),
  agentExecutionManager.executeAsync(subAgent3),
];

// 等待所有任务完成
await agentExecutionManager.waitForAll(taskIds.map(r => r.taskId));
```

---

## 6. 优势分析

### 6.1 统一架构的优势

1. **代码复用**:
   - Pool/Queue 逻辑只需实现一次
   - 减少重复代码
   - 统一维护

2. **一致的行为**:
   - Graph 和 Agent 有相同的并发控制
   - 相同的资源管理策略
   - 相同的错误处理

3. **易于扩展**:
   - 新增执行类型只需实现 Executor
   - 复用现有基础设施
   - 降低扩展成本

4. **统一监控**:
   - TaskRegistry 统一管理所有任务
   - 统一的统计信息
   - 统一的状态查询

### 6.2 差异化实现的优势

1. **保留特性**:
   - Graph 保留图遍历特性
   - Agent 保留迭代循环特性
   - 各自优化执行逻辑

2. **独立配置**:
   - Graph 和 Agent 可以独立配置
   - 不同的并发策略
   - 不同的资源限制

3. **隔离故障**:
   - Graph 故障不影响 Agent
   - Agent 故障不影响 Graph
   - 提高系统稳定性

---

## 7. 实施建议

### 7.1 优先级

| 任务 | 优先级 | 工作量 |
|------|--------|--------|
| 泛化 ThreadPoolService | 高 | 中 |
| 泛化 TaskQueueManager | 高 | 中 |
| 创建 AgentExecutionManager | 高 | 中 |
| 重构 TriggeredSubworkflowManager | 中 | 低 |
| 更新文档和测试 | 中 | 中 |

### 7.2 实施步骤

1. **第一阶段**（1-2 天）:
   - 泛化 ThreadPoolService
   - 泛化 TaskQueueManager
   - 移动到 core 模块

2. **第二阶段**（1-2 天）:
   - 创建 AgentExecutionManager
   - 创建 AgentExecutorFactory
   - 集成到 DI 容器

3. **第三阶段**（1 天）:
   - 重构 TriggeredSubworkflowManager
   - 创建 ThreadExecutionManager
   - 更新测试

4. **第四阶段**（1 天）:
   - 更新文档
   - 添加使用示例
   - 性能测试

### 7.3 向后兼容

1. **保留别名**:
   ```typescript
   // 向后兼容
   export { ExecutionPoolService as ThreadPoolService };
   export { ExecutionQueueManager as TaskQueueManager };
   ```

2. **保留现有 API**:
   - TriggeredSubworkflowManager API 不变
   - AgentLoopExecutor API 不变

3. **渐进式迁移**:
   - 新功能使用新架构
   - 旧功能逐步迁移

---

## 8. 总结

### 8.1 核心结论

1. **Agent 确实需要并发支持**:
   - Sub-Agent 与主 Agent 并发
   - 多 Agent 在不同 worktree 并发
   - 批量任务处理

2. **应该统一架构**:
   - 共享 Pool/Queue 基础设施
   - 减少重复代码
   - 统一行为和监控

3. **保留差异化实现**:
   - 各自的执行器实现
   - 独立的配置
   - 故障隔离

### 8.2 设计原则

1. **统一抽象**: 共享基础设施
2. **差异实现**: 保留各自特性
3. **资源复用**: 减少重复代码
4. **向后兼容**: 渐进式迁移

### 8.3 后续工作

1. 实施泛化重构
2. 创建 Agent 执行管理器
3. 更新文档和测试
4. 性能测试和优化

---

**文档版本**: 2.0
**更新日期**: 2026-04-06
**作者**: CodeArts Agent
