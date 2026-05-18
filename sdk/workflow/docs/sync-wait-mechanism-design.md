# SYNC 节点等待机制设计方案

## 一、问题分析

### 1.1 当前挑战

SYNC 节点需要实现以下核心功能：
1. **查找源执行** - 根据 `sourcePathId` 定位需要同步的兄弟分支执行实例
2. **等待完成** - 可选等待源分支执行完成
3. **超时控制** - 支持可配置的超时时间
4. **变量同步** - 从源分支深拷贝变量到目标分支

### 1.2 现有架构分析

#### 已有的基础设施

✅ **超时工具** (`sdk/core/utils/timeout/`)
- `createTimeoutPromise()` - 基于 Promise.race 的超时包装
- `withTimeout()` - 函数执行的超时控制
- `combineTimeoutWithSignal()` - 结合 AbortSignal 的超时
- `delay()` - 支持中断的延迟

✅ **事件系统** (`sdk/core/registry/`)
- `EventRegistry` - 全局事件注册表，管理每个执行的 EventEmitter
- `ExecutionEventEmitter` - 每个执行独立的事件发射器
- `waitFor()` - 基于事件的等待机制，支持超时和过滤
- `emit()` - 事件发射，自动通知所有监听器

✅ **事件等待封装** (`sdk/workflow/execution/utils/event/event-waiter.ts`)
- `waitForWorkflowExecutionCompleted()` - 等待执行完成
- `waitForMultipleWorkflowExecutionsCompleted()` - 等待多个执行完成
- `waitForAnyWorkflowExecutionCompleted()` - 等待任一执行完成
- 底层使用 `eventManager.waitFor()` + 超时控制

✅ **Fork-Join 实现** (`workflow-operations.ts`)
- Fork 时创建子执行实体，存储在 registry 中
- Join 时使用 `waitForCompletion()` 等待子执行
- 通过 `workflowExecutionRegistry.get(executionId)` 获取执行实体

#### 存在的问题

❌ **缺少跨执行访问机制**
- SYNC 节点在子执行内部运行，无法直接访问父执行或其他兄弟执行
- 没有从 `forkPathId` 反查 `executionId` 的映射关系

❌ **Join 实现不完整**
- Join 只合并主路径的消息历史（第 232-280 行）
- **未处理变量输出** - 文档提到需要支持 `variableOutputs`，但当前实现仅简单聚合 output
- 变量传递仍依赖隐式的 global 共享（阶段1已修复，但 Join 导出未实现）

---

## 二、设计方案

### 2.1 核心设计原则

1. **显式协调** - 通过父执行作为协调中心，避免子执行间直接耦合
2. **事件驱动** - 复用现有的事件等待机制，避免轮询
3. **超时复用** - 统一使用 `sdk/core/utils/timeout` 的工具函数
4. **完全隔离** - 保持阶段1建立的完全隔离模型

### 2.2 架构改进方案

#### 方案 A：通过父执行协调（推荐）

```
┌─────────────────────────────────────────────┐
│         Parent Execution (FORK)             │
│  ┌───────────────────────────────────────┐  │
│  │  forkContext: {                       │  │
│  │    pathId -> executionId mapping      │  │
│  │    barrier: SyncBarrier instance      │  │
│  │  }                                    │  │
│  └───────────────────────────────────────┘  │
│           ↑              ↑                  │
│     Branch A        Branch B                │
│  (targetPath)   (sourcePath)                │
│       │              │                      │
│       └── SYNC node ─┘                     │
│          查询父执行的映射                    │
└─────────────────────────────────────────────┘
```

**关键组件：**

1. **ForkContext 扩展** - 在父执行中存储路径映射
2. **SyncBarrier** - 同步屏障，管理跨分支的等待
3. **ExecutionHierarchy 增强** - 提供父子执行访问能力

#### 方案 B：通过 Registry 查询

利用现有的 `WorkflowExecutionRegistry`，通过遍历找到匹配的 `forkPathId`。

**缺点：**
- 性能差（需要遍历所有执行）
- 无法区分不同层级的 fork
- 耦合度高

**结论：** 不推荐

#### 方案 C：在执行上下文中传递引用

创建子执行时，将父执行引用和兄弟执行 ID 列表传递给子执行。

**优点：**
- 直接访问，性能好
- 实现简单

**缺点：**
- 增加执行实体的复杂度
- 可能形成循环引用

**结论：** 可作为方案 A 的补充

---

### 2.3 推荐实施方案（方案 A + C 混合）

#### 步骤 1：扩展 ForkContext

在父执行中维护 fork 分支的路径映射：

```typescript
// 在 WorkflowExecution 或 ExecutionState 中添加
interface ForkContext {
  /** forkPathId -> executionId 映射 */
  pathToExecutionMap: Map<string, string>;
  
  /** 父执行 ID 引用 */
  parentExecutionId?: string;
  
  /** 当前分支的 forkPathId */
  currentForkPathId?: string;
}
```

**修改位置：**
- `workflow-execution-builder.ts` - 创建子执行时初始化 ForkContext
- `fork-handler.ts` - 记录每个分支的 executionId

#### 步骤 2：实现 SyncBarrier

```typescript
/**
 * SyncBarrier - 同步屏障，管理跨分支的变量同步
 * 
 * 职责：
 * - 维护 forkPathId 到 executionId 的映射
 * - 提供基于事件的等待机制
 * - 协调多个 SYNC 节点的并发访问
 */
class SyncBarrier {
  private pathToExecutionMap: Map<string, string>;
  private eventManager: EventRegistry;
  private parentExecutionId: string;
  
  constructor(
    parentExecutionId: string,
    eventManager: EventRegistry,
    initialMapping?: Map<string, string>
  ) {
    this.parentExecutionId = parentExecutionId;
    this.eventManager = eventManager;
    this.pathToExecutionMap = initialMapping || new Map();
  }
  
  /**
   * 注册分支执行
   */
  registerBranch(forkPathId: string, executionId: string): void {
    this.pathToExecutionMap.set(forkPathId, executionId);
  }
  
  /**
   * 获取分支执行 ID
   */
  getExecutionId(forkPathId: string): string | undefined {
    return this.pathToExecutionMap.get(forkPathId);
  }
  
  /**
   * 等待指定分支完成
   * 
   * @param forkPathId 分支路径 ID
   * @param timeout 超时时间（毫秒），0 表示无超时
   * @returns 完成的执行实体
   */
  async waitForBranchCompletion(
    forkPathId: string,
    timeout: number = 0
  ): Promise<WorkflowExecutionEntity> {
    const executionId = this.pathToExecutionMap.get(forkPathId);
    
    if (!executionId) {
      throw new RuntimeValidationError(
        `Branch not found for forkPathId: ${forkPathId}`,
        {
          operation: "SyncBarrier.waitForBranchCompletion",
          field: "forkPathId",
          context: { forkPathId, availablePaths: Array.from(this.pathToExecutionMap.keys()) }
        }
      );
    }
    
    // 复用现有的事件等待机制
    const timeoutMs = timeout > 0 ? timeout : undefined;
    
    try {
      await waitForWorkflowExecutionCompleted(
        this.eventManager,
        executionId,
        timeoutMs ?? WAIT_FOREVER
      );
      
      // 从 registry 获取执行实体
      const registry = /* 通过 DI 获取 */;
      const entity = registry.get(executionId);
      
      if (!entity) {
        throw new Error(`Execution entity not found: ${executionId}`);
      }
      
      return entity;
    } catch (error) {
      if (isTimeoutError(error)) {
        throw new ExecutionError(
          `Timeout waiting for branch completion: ${forkPathId}`,
          undefined,
          this.parentExecutionId,
          { forkPathId, executionId, timeout }
        );
      }
      throw error;
    }
  }
}
```

#### 步骤 3：修改 Fork Handler

在 `fork-handler.ts` 中注册分支映射：

```typescript
export async function forkHandler(...) {
  // ... 现有代码 ...
  
  // Step 1: 创建所有分支执行实体
  branchCreations = await Promise.all(
    forkPaths.map(async (path) => {
      const buildResult = await builder.createChildExecution(...);
      
      // 【新增】注册到父执行的 SyncBarrier
      const syncBarrier = workflowExecutionEntity.getSyncBarrier();
      if (syncBarrier) {
        syncBarrier.registerBranch(path.pathId, buildResult.workflowExecutionEntity.id);
      }
      
      return { pathId: path.pathId, branchEntity: buildResult.workflowExecutionEntity };
    })
  );
  
  // ... 继续执行 ...
}
```

#### 步骤 4：完善 Sync Handler

```typescript
export async function syncHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode
): Promise<unknown> {
  const config = node.config as SyncNodeConfig;
  
  // Step 1: 获取父执行的 SyncBarrier
  const parentExecutionId = workflowExecutionEntity.getParentExecutionId();
  if (!parentExecutionId) {
    throw new RuntimeValidationError(
      "SYNC node must be executed within a fork branch",
      { operation: "syncHandler", context: { nodeId: node.id } }
    );
  }
  
  const registry = /* 通过 DI 获取 WorkflowExecutionRegistry */;
  const parentEntity = registry.get(parentExecutionId);
  if (!parentEntity) {
    throw new Error(`Parent execution not found: ${parentExecutionId}`);
  }
  
  const syncBarrier = parentEntity.getSyncBarrier();
  if (!syncBarrier) {
    throw new Error("SyncBarrier not initialized in parent execution");
  }
  
  // Step 2: 查找源执行 ID
  const sourceExecutionId = syncBarrier.getExecutionId(config.sourcePathId);
  if (!sourceExecutionId) {
    throw new RuntimeValidationError(
      `Source branch not found for forkPathId: ${config.sourcePathId}`,
      {
        operation: "syncHandler",
        field: "sourcePathId",
        context: { 
          nodeId: node.id,
          sourcePathId: config.sourcePathId,
          availablePaths: syncBarrier.getAvailablePaths()
        }
      }
    );
  }
  
  // Step 3: 等待源执行完成（如果配置）
  let sourceEntity: WorkflowExecutionEntity;
  
  if (config.waitForCompletion ?? true) {
    const timeout = config.timeout ?? 0;
    
    logger.debug("Waiting for source branch completion", {
      nodeId: node.id,
      sourcePathId: config.sourcePathId,
      sourceExecutionId,
      timeout,
    });
    
    // 使用 SyncBarrier 的等待方法（内部复用 eventManager.waitFor）
    sourceEntity = await syncBarrier.waitForBranchCompletion(
      config.sourcePathId,
      timeout > 0 ? timeout * 1000 : 0  // 转换为毫秒
    );
  } else {
    // 不等待，直接获取当前状态
    sourceEntity = registry.get(sourceExecutionId);
    if (!sourceEntity) {
      throw new Error(`Source execution entity not found: ${sourceExecutionId}`);
    }
  }
  
  // Step 4: 导入变量（深拷贝）
  if (config.variableMappings && config.variableMappings.length > 0) {
    logger.debug("Importing variables from source to target", {
      nodeId: node.id,
      mappingCount: config.variableMappings.length,
    });
    
    workflowExecutionEntity.variableStateManager.importVariables(
      sourceEntity.variableStateManager,
      config.variableMappings
    );
    
    logger.info("SYNC completed successfully", {
      nodeId: node.id,
      importedVariableCount: config.variableMappings.length,
    });
  }
  
  return {
    synced: true,
    sourcePathId: config.sourcePathId,
    sourceExecutionId,
    variableCount: config.variableMappings?.length || 0,
  };
}
```

---

## 三、事件系统设计

### 3.1 新增事件类型

为了支持 SYNC 节点的监控和调试，建议添加以下事件：

```typescript
// 在 types/src/events/workflow-events.ts 中添加

/**
 * SYNC 节点开始事件
 */
export interface SyncStartedEvent extends BaseEvent {
  type: "NODE_SYNC_STARTED";
  nodeId: string;
  sourcePathId: string;
  targetPathId?: string;
  waitForCompletion: boolean;
  timeout?: number;
}

/**
 * SYNC 节点完成事件
 */
export interface SyncCompletedEvent extends BaseEvent {
  type: "NODE_SYNC_COMPLETED";
  nodeId: string;
  sourcePathId: string;
  sourceExecutionId: string;
  variableCount: number;
  duration: number;
}

/**
 * SYNC 节点失败事件
 */
export interface SyncFailedEvent extends BaseEvent {
  type: "NODE_SYNC_FAILED";
  nodeId: string;
  sourcePathId: string;
  error: string;
}
```

### 3.2 事件发射时机

在 `sync-handler.ts` 中：

```typescript
// 开始时发射
await emit(eventManager, buildSyncStartedEvent({
  executionId: workflowExecutionEntity.id,
  workflowId: workflowExecutionEntity.getWorkflowId(),
  nodeId: node.id,
  sourcePathId: config.sourcePathId,
  targetPathId: config.targetPathId,
  waitForCompletion: config.waitForCompletion ?? true,
  timeout: config.timeout,
}));

const startTime = now();

try {
  // ... 执行同步逻辑 ...
  
  // 成功时发射
  await emit(eventManager, buildSyncCompletedEvent({
    executionId: workflowExecutionEntity.id,
    workflowId: workflowExecutionEntity.getWorkflowId(),
    nodeId: node.id,
    sourcePathId: config.sourcePathId,
    sourceExecutionId,
    variableCount: config.variableMappings?.length || 0,
    duration: diffTimestamp(startTime, now()),
  }));
} catch (error) {
  // 失败时发射
  await emit(eventManager, buildSyncFailedEvent({
    executionId: workflowExecutionEntity.id,
    workflowId: workflowExecutionEntity.getWorkflowId(),
    nodeId: node.id,
    sourcePathId: config.sourcePathId,
    error: getErrorMessage(error),
  }));
  throw error;
}
```

---

## 四、Join 节点的改进需求

### 4.1 当前问题

查看 `workflow-operations.ts` 的 `join()` 函数（第 159-289 行）：

**已完成：**
- ✅ 等待策略（ALL_COMPLETED, ANY_COMPLETED 等）
- ✅ 超时控制
- ✅ 主路径消息历史合并

**缺失：**
- ❌ **变量输出映射** - 没有实现 `variableOutputs` 配置
- ❌ **多分支消息合并** - 只合并主路径，其他分支丢失
- ❌ **自定义结果合并** - 只能简单按 executionId 聚合

### 4.2 改进方案

#### 扩展 JoinNodeConfig

```typescript
// packages/types/src/node/configs/fork-join-configs.ts

export interface JoinNodeConfig {
  forkPathIds: ID[];
  joinStrategy: 'ALL_COMPLETED' | 'ANY_COMPLETED' | 'ALL_FAILED' | 'ANY_FAILED' | 'SUCCESS_COUNT_THRESHOLD';
  threshold?: number;
  timeout?: number;
  mainPathId: ID;
  
  // 【新增】显式变量输出映射
  variableOutputs?: WorkflowVariableOutput[];
  
  // 【新增】消息历史合并策略
  messageMergeStrategy?: 'MAIN_ONLY' | 'ALL_BRANCHES' | 'CUSTOM';
}
```

#### 修改 join() 函数

```typescript
export async function join(
  childExecutionIds: string[],
  joinStrategy: JoinStrategy,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  mainPathId: string,
  timeout: number = 0,
  parentExecutionId?: string,
  eventManager?: EventRegistry,
  variableOutputs?: WorkflowVariableOutput[],  // 【新增参数】
): Promise<JoinResult> {
  // ... 现有等待逻辑 ...
  
  // Step 4: 合并结果
  const output = mergeResults(completedExecutions, joinStrategy);
  
  // Step 5: 合并消息历史（改进）
  if (parentExecutionId) {
    const parentEntity = workflowExecutionRegistry.get(parentExecutionId);
    if (parentEntity) {
      // 根据策略合并消息
      const mergeStrategy = /* 从配置读取 */ 'MAIN_ONLY';
      
      switch (mergeStrategy) {
        case 'MAIN_ONLY':
          // 现有逻辑：只合并主路径
          mergeMainPathMessages(parentEntity, completedExecutions, mainPathId, workflowExecutionRegistry);
          break;
          
        case 'ALL_BRANCHES':
          // 新逻辑：合并所有分支（按完成顺序）
          mergeAllBranchMessages(parentEntity, completedExecutions, workflowExecutionRegistry);
          break;
          
        case 'CUSTOM':
          // 预留：允许自定义合并函数
          break;
      }
    }
  }
  
  // Step 6: 【新增】导出变量到父执行
  if (parentExecutionId && variableOutputs && variableOutputs.length > 0) {
    const parentEntity = workflowExecutionRegistry.get(parentExecutionId);
    if (parentEntity) {
      // 找到主路径执行
      const mainExecution = completedExecutions.find(
        exec => exec.forkJoinContext?.forkPathId === mainPathId
      );
      
      if (mainExecution) {
        const mainEntity = workflowExecutionRegistry.get(mainExecution.id);
        if (mainEntity) {
          // 使用 VariableManager 的 exportVariables 方法（深拷贝）
          mainEntity.variableStateManager.exportVariables(
            parentEntity.variableStateManager,
            variableOutputs
          );
          
          logger.info("Join: Variables exported to parent", {
            parentExecutionId,
            variableCount: variableOutputs.length,
          });
        }
      }
    }
  }
  
  return { success: true, output, completedExecutions, failedExecutions };
}
```

---

## 五、实施任务清单

### 阶段 2A：SYNC 等待机制实现

**任务 2A.1：扩展执行实体**
- [ ] 在 `WorkflowExecutionEntity` 中添加 `getSyncBarrier()` 方法
- [ ] 在 `WorkflowExecution` 数据模型中添加 `forkContext` 字段
- [ ] 实现 `getParentExecutionId()` 方法

**任务 2A.2：实现 SyncBarrier**
- [ ] 创建 `sdk/workflow/execution/barriers/sync-barrier.ts`
- [ ] 实现路径映射管理
- [ ] 实现基于事件的等待逻辑
- [ ] 集成超时控制（复用 `sdk/core/utils/timeout`）

**任务 2A.3：修改 Fork Handler**
- [ ] 在 `fork-handler.ts` 中初始化 SyncBarrier
- [ ] 注册每个分支的路径映射
- [ ] 将 SyncBarrier 附加到父执行实体

**任务 2A.4：完善 Sync Handler**
- [ ] 替换占位符 `findSourceExecution()` 为实际实现
- [ ] 替换占位符 `waitForSourceCompletion()` 为 SyncBarrier 调用
- [ ] 添加事件发射（SYNC_STARTED, SYNC_COMPLETED, SYNC_FAILED）
- [ ] 编写集成测试

### 阶段 2B：Join 节点增强

**任务 2B.1：扩展 JoinNodeConfig**
- [ ] 添加 `variableOutputs` 字段
- [ ] 添加 `messageMergeStrategy` 字段
- [ ] 更新 Zod schema 验证

**任务 2B.2：实现变量导出**
- [ ] 修改 `workflow-operations.ts` 的 `join()` 函数
- [ ] 使用 `exportVariables()` 导出主分支变量到父执行
- [ ] 添加相关事件发射

**任务 2B.3：优化消息合并**
- [ ] 实现 `mergeAllBranchMessages()` 函数
- [ ] 支持配置化合并策略
- [ ] 处理消息顺序和去重

**任务 2B.4：测试与文档**
- [ ] 编写 Join variableOutputs 的集成测试
- [ ] 编写消息合并策略的测试
- [ ] 更新文档和使用示例

---

## 六、技术细节

### 6.1 超时控制复用

统一使用 `sdk/core/utils/timeout` 的工具：

```typescript
import { createTimeoutPromise, isTimeoutError } from '@wf-agent/sdk/core/utils/timeout';

// 方式 1：包装 Promise
async function waitForWithTimeout<T>(
  promise: Promise<T>,
  timeout: number,  // 毫秒
  message: string
): Promise<T> {
  if (timeout <= 0) {
    return promise;  // 无超时
  }
  
  return createTimeoutPromise(promise, timeout, message);
}

// 方式 2：使用 withTimeout
const result = await withTimeout(
  () => syncBarrier.waitForBranchCompletion(pathId, 0),
  timeoutMs,
  { message: `SYNC timeout for branch: ${pathId}` }
);
```

### 6.2 事件等待流程

```
SYNC Node Execution
    ↓
获取父执行的 SyncBarrier
    ↓
查询 sourcePathId → executionId 映射
    ↓
[可选] 等待源执行完成
    ↓
eventManager.waitFor('WORKFLOW_EXECUTION_COMPLETED', executionId, timeout)
    ↓
ExecutionEventEmitter.once() 注册一次性监听器
    ↓
源执行完成后触发事件
    ↓
Promise 解析，返回执行实体
    ↓
importVariables() 深拷贝变量
    ↓
发射 SYNC_COMPLETED 事件
```

### 6.3 错误处理

```typescript
try {
  // 同步逻辑
} catch (error) {
  if (isTimeoutError(error)) {
    // 超时错误
    throw new ExecutionError(
      `SYNC timeout: ${config.sourcePathId}`,
      undefined,
      workflowExecutionEntity.id,
      { sourcePathId: config.sourcePathId, timeout: config.timeout }
    );
  }
  
  if (error instanceof RuntimeValidationError) {
    // 验证错误（路径不存在、配置错误等）
    throw error;
  }
  
  // 其他错误
  throw new ExecutionError(
    `SYNC failed: ${getErrorMessage(error)}`,
    undefined,
    workflowExecutionEntity.id,
    { originalError: error }
  );
}
```

---

## 七、总结

### 核心设计要点

1. **协调机制** - 通过父执行的 SyncBarrier 管理跨分支同步
2. **事件驱动** - 复用 `eventManager.waitFor()` 实现高效等待
3. **超时复用** - 统一使用 `sdk/core/utils/timeout` 工具
4. **完全隔离** - 保持阶段1的深拷贝隔离模型
5. **渐进增强** - 先实现基础等待，再优化 Join 的变量导出

### 下一步行动

1. **立即开始** - 实施阶段 2A（SYNC 等待机制）
2. **并行进行** - 设计 SyncBarrier 的单元测试
3. **后续跟进** - 阶段 2B（Join 增强）依赖于 2A 完成

### 预期收益

- ✅ 消除竞态条件，实现确定性数据流
- ✅ 显式的跨分支数据传递契约
- ✅ 完善的超时控制和错误处理
- ✅ 可扩展的事件监控和调试能力
