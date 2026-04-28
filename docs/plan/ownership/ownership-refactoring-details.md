# 所有权体系重构详解

## 文档说明

本文档提供实际的代码示例、问题演示和改进方案的详细实现细节。

---

## 第一部分：问题演示

### 问题1：TriggeredSubworkflowManager 的多服务冲突

#### 当前问题代码

**triggered-subworkflow-manager.ts - executeTriggeredSubgraph() 方法**

```typescript
async executeTriggeredSubgraph(
  task: TriggeredSubgraphTask
): Promise<ExecutedSubgraphResult | TaskSubmissionResult> {
  // ... 准备代码 ...
  
  const subgraphEntity = await this.createSubgraphContext(task, input);
  
  // 问题1：注册到多个地方
  this.threadRegistry.register(subgraphEntity);      // [注册点1]
  // ... 
  const taskId = this.taskRegistry.register(subgraphEntity, this, timeout);  // [注册点2]
  
  // 这时 subgraphEntity 被以下持有：
  // - ThreadRegistry（map中的引用）
  // - TaskRegistry（TaskInfo中的引用）
  // - activeTasks（可能的引用）
  // - 本方法的局部变量引用
}
```

**handleSubgraphCompleted() 方法**

```typescript
private handleSubgraphCompleted(threadId: string, result: ExecutedSubgraphResult): void {
  // 问题2：不清晰的清理顺序
  
  // 步骤1：从 ThreadRegistry 获取
  const subgraphEntity = this.threadRegistry.get(threadId);  // ⚠️ [获取点1]
  
  if (subgraphEntity) {
    this.unregisterParentChildRelationship(subgraphEntity);
  }
  
  // 步骤2：从 TaskRegistry 查找和删除
  const taskInfo = this.taskRegistry.getAll().find(
    t => t.threadEntity.getThreadId() === threadId  // ⚠️ [获取点2] - 这里仍在使用
  );
  
  if (taskInfo) {
    this.taskRegistry.delete(taskInfo.id);  // ⚠️ [删除点1]
  }
  
  // 步骤3：清理本地记录
  this.activeTasks.delete(threadId);  // ⚠️ [删除点2]
  
  // 问题3：threadRegistry 没有被清理！
  // ThreadRegistry 中仍然持有 subgraphEntity 的引用
  // 如果某个外部调用者此时调用 threadRegistry.delete(threadId)
  // 就会导致数据不一致
}
```

#### 问题可视化

```
时间流  操作                           ThreadRegistry  TaskRegistry  activeTasks
─────────────────────────────────────────────────────────────────────────────
t0      executeTriggeredSubgraph()
        register(entity)              [entity]        -               -
        taskRegistry.register()       [entity]        [entity]        -
        executeAsync()                [entity]        [entity]        [taskId]

t1      handleSubgraphCompleted()
        threadRegistry.get()          [entity] ✓       [entity]        [taskId]
        taskRegistry.delete()         [entity]        (empty)         [taskId]
        activeTasks.delete()          [entity]        (empty)         (empty)

t2      外部调用 threadRegistry.delete(threadId)  ⚠️ 风险！
        虽然 TaskRegistry 已清理，但 ThreadRegistry 仍有引用
        如果同时有其他操作访问该 entity，会出现冲突
```

### 问题2：Graph 引用隔离问题

#### 当前设计

**thread-builder.ts**

```typescript
async buildFromPreprocessedGraph(
  preprocessedGraph: PreprocessedGraph,
  options: ThreadOptions = {}
): Promise<ThreadEntity> {
  // ... 验证 ...
  
  const threadGraphData = preprocessedGraph;  // ⚠️ 直接赋值，非深拷贝
  
  const thread: Thread = {
    id: threadId,
    workflowId: preprocessedGraph.workflowId,
    workflowVersion: preprocessedGraph.workflowVersion,
    status: 'CREATED' as ThreadStatus,
    currentNodeId: startNode.id,
    graph: threadGraphData,  // ⚠️ 引用同一对象
    // ...
  };
  
  // ...
  return new ThreadEntity(thread, executionState);
}
```

#### 问题情景

```
场景：多个线程，工作流版本升级

时间线：
  
  t0: 构建线程1
      Thread1 = ThreadBuilder.build('workflow-v1')
      Thread1.graph = PreprocessedGraph@v1
      
  t1: 构建线程2
      Thread2 = ThreadBuilder.build('workflow-v1')
      Thread2.graph = PreprocessedGraph@v1  // 同一对象！
      
  t2: 工作流升级，注册新版本
      GraphRegistry.register('workflow-v2', newGraph)
      // PreprocessedGraph@v1 对象本身未变，但可能被垃圾回收
      
  t3: 线程1执行
      navigator = GraphNavigator(Thread1.graph)
      // 问题：如果 GraphRegistry 被清理了 v1
      // PreprocessedGraph 可能无法被安全访问
      
  t4: 线程2 执行
      navigator = GraphNavigator(Thread2.graph)
      // 同样的问题
```

#### 多线程版本隔离问题

```typescript
// 情景：图的元数据被修改

const preprocessedGraph = graphRegistry.get('workflow-1');

// Thread1 和 Thread2 都引用这个 preprocessedGraph
Thread1.graph = preprocessedGraph;
Thread2.graph = preprocessedGraph;

// 如果某个地方修改了 preprocessedGraph（虽然不应该，但可能发生）
preprocessedGraph.triggers = newTriggers;  // ⚠️ Thread1 和 Thread2 都看到修改

// 这违反了线程隔离原则
// 每个线程应该有自己的图状态副本
```

---

## 第二部分：改进方案实现

### 方案1：ThreadLifecycleCoordinator 实现

#### 完整实现

**sdk/core/execution/coordinators/thread-lifecycle-coordinator.ts**

```typescript
/**
 * ThreadLifecycleCoordinator - 线程生命周期协调器
 * 
 * 职责：
 * - 单一入口点：所有 ThreadEntity 的创建、执行、清理
 * - 生命周期事件：触发和协调各服务的响应
 * - 保护机制：防止 Use-After-Free
 * - 可观测性：日志和追踪
 */

import type { ThreadOptions, ThreadResult } from '@modular-agent/types';
import type { ThreadEntity } from '../entities/thread-entity.js';
import type { ThreadBuilder } from './thread-builder.js';
import type { ThreadRegistry } from '../services/thread-registry.js';
import type { TaskRegistry } from '../services/task-registry.js';
import type { ExecutionContext } from '../execution/execution-context.js';
import { EventEmitter } from '../utils/event-emitter.js';
import { createContextualLogger } from '../../utils/contextual-logger.js';

const logger = createContextualLogger();

/**
 * 线程生命周期事件
 */
export interface ThreadLifecycleEvent {
  type: 'CREATED' | 'STARTED' | 'PAUSED' | 'RESUMED' | 'COMPLETED' | 'FAILED' | 'CLEANING' | 'DELETED';
  threadId: string;
  timestamp: number;
  workflowId?: string;
  data?: Record<string, any>;
  error?: Error;
}

/**
 * 清理检查点回调
 */
export interface CleanupCheckpoint {
  name: string;
  handler: (threadId: string) => Promise<void>;
  timeout: number;  // 毫秒
}

/**
 * ThreadLifecycleCoordinator 实现
 */
export class ThreadLifecycleCoordinator {
  private threadBuilder: ThreadBuilder;
  private threadRegistry: ThreadRegistry;
  private taskRegistry: TaskRegistry;
  private threadExecutor: any;
  
  private lifecycleEmitter = new EventEmitter<ThreadLifecycleEvent>();
  private cleanupCheckpoints: Map<string, CleanupCheckpoint> = new Map();
  private pendingCleanups: Map<string, Promise<void>> = new Map();
  
  /**
   * 构造函数 - 通过 DI 注入依赖
   */
  constructor(
    threadBuilder: ThreadBuilder,
    threadRegistry: ThreadRegistry,
    taskRegistry: TaskRegistry,
    threadExecutor: any
  ) {
    this.threadBuilder = threadBuilder;
    this.threadRegistry = threadRegistry;
    this.taskRegistry = taskRegistry;
    this.threadExecutor = threadExecutor;
    
    // 注册默认的清理检查点
    this.registerDefaultCheckpoints();
  }
  
  /**
   * 创建线程
   * 
   * @param workflowId 工作流ID
   * @param options 线程选项
   * @returns ThreadEntity 实例
   */
  async createThread(
    workflowId: string,
    options: ThreadOptions = {}
  ): Promise<ThreadEntity> {
    logger.debug(`Creating thread for workflow: ${workflowId}`);
    
    try {
      // 步骤1：通过 ThreadBuilder 创建 ThreadEntity
      const threadEntity = await this.threadBuilder.build(workflowId, options);
      const threadId = threadEntity.getThreadId();
      
      // 步骤2：注册到 ThreadRegistry
      this.threadRegistry.register(threadEntity);
      logger.debug(`Thread registered: ${threadId}`);
      
      // 步骤3：触发 CREATED 事件
      this.emitEvent({
        type: 'CREATED',
        threadId,
        workflowId,
        timestamp: Date.now()
      });
      
      return threadEntity;
    } catch (error) {
      logger.error(`Failed to create thread for workflow ${workflowId}:`, error);
      throw error;
    }
  }
  
  /**
   * 开始执行线程
   * 
   * @param threadId 线程ID
   * @returns 执行结果
   */
  async executeThread(threadId: string): Promise<ThreadResult> {
    logger.debug(`Starting execution for thread: ${threadId}`);
    
    const threadEntity = this.threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new Error(`Thread not found: ${threadId}`);
    }
    
    try {
      // 触发 STARTED 事件
      this.emitEvent({
        type: 'STARTED',
        threadId,
        timestamp: Date.now()
      });
      
      // 执行线程
      const result = await this.threadExecutor.executeThread(threadEntity);
      
      // 触发 COMPLETED 事件
      this.emitEvent({
        type: 'COMPLETED',
        threadId,
        timestamp: Date.now(),
        data: { result }
      });
      
      return result;
    } catch (error) {
      logger.error(`Thread execution failed: ${threadId}`, error);
      
      // 触发 FAILED 事件
      this.emitEvent({
        type: 'FAILED',
        threadId,
        timestamp: Date.now(),
        error: error as Error
      });
      
      throw error;
    }
  }
  
  /**
   * 清理线程
   * 
   * 执行流程：
   * 1. 触发 CLEANING 事件
   * 2. 执行所有已注册的清理检查点
   * 3. 从各注册表中删除
   * 4. 触发 DELETED 事件
   * 
   * @param threadId 线程ID
   * @param reason 清理原因
   */
  async cleanupThread(threadId: string, reason: string = 'normal'): Promise<void> {
    logger.debug(`Cleaning up thread: ${threadId} (reason: ${reason})`);
    
    // 避免重复清理
    if (this.pendingCleanups.has(threadId)) {
      logger.debug(`Cleanup already in progress for thread: ${threadId}`);
      return this.pendingCleanups.get(threadId)!;
    }
    
    const cleanupPromise = this._performCleanup(threadId, reason);
    this.pendingCleanups.set(threadId, cleanupPromise);
    
    try {
      await cleanupPromise;
    } finally {
      this.pendingCleanups.delete(threadId);
    }
  }
  
  /**
   * 执行清理操作（内部方法）
   */
  private async _performCleanup(threadId: string, reason: string): Promise<void> {
    try {
      // 步骤1：触发 CLEANING 事件（告知所有订阅者）
      this.emitEvent({
        type: 'CLEANING',
        threadId,
        timestamp: Date.now(),
        data: { reason }
      });
      
      // 步骤2：执行所有清理检查点（顺序执行）
      await this.executeCleanupCheckpoints(threadId);
      
      // 步骤3：从 TaskRegistry 清理任务记录
      const tasks = this.taskRegistry.getAll()
        .filter(t => t.threadEntity.getThreadId() === threadId);
      
      for (const task of tasks) {
        this.taskRegistry.delete(task.id);
        logger.debug(`Task deleted: ${task.id}`);
      }
      
      // 步骤4：从 ThreadRegistry 删除线程
      const removed = this.threadRegistry.delete(threadId);
      if (removed) {
        logger.debug(`Thread removed from registry: ${threadId}`);
      } else {
        logger.warn(`Thread not found in registry: ${threadId}`);
      }
      
      // 步骤5：触发 DELETED 事件
      this.emitEvent({
        type: 'DELETED',
        threadId,
        timestamp: Date.now(),
        data: { reason }
      });
      
    } catch (error) {
      logger.error(`Cleanup failed for thread ${threadId}:`, error);
      throw error;
    }
  }
  
  /**
   * 执行所有清理检查点
   */
  private async executeCleanupCheckpoints(threadId: string): Promise<void> {
    const checkpoints = Array.from(this.cleanupCheckpoints.values());
    
    for (const checkpoint of checkpoints) {
      try {
        logger.debug(`Executing cleanup checkpoint: ${checkpoint.name} (threadId: ${threadId})`);
        
        // 带超时的执行
        await this.executeWithTimeout(
          checkpoint.handler(threadId),
          checkpoint.timeout,
          `Cleanup checkpoint ${checkpoint.name} timed out`
        );
        
        logger.debug(`Cleanup checkpoint completed: ${checkpoint.name}`);
      } catch (error) {
        logger.error(`Cleanup checkpoint failed: ${checkpoint.name}`, error);
        // 继续执行其他检查点，不要中断
      }
    }
  }
  
  /**
   * 带超时的执行
   */
  private executeWithTimeout<T>(
    promise: Promise<T>,
    timeout: number,
    timeoutMessage: string
  ): Promise<T> {
    return Promise.race([
      promise,
      new Promise<T>((_, reject) =>
        setTimeout(() => reject(new Error(timeoutMessage)), timeout)
      )
    ]);
  }
  
  /**
   * 注册清理检查点
   * 
   * @param name 检查点名称
   * @param handler 处理函数
   * @param timeout 超时时间（毫秒）
   */
  registerCleanupCheckpoint(
    name: string,
    handler: (threadId: string) => Promise<void>,
    timeout: number = 5000
  ): void {
    this.cleanupCheckpoints.set(name, { name, handler, timeout });
    logger.debug(`Cleanup checkpoint registered: ${name}`);
  }
  
  /**
   * 注册默认的清理检查点
   */
  private registerDefaultCheckpoints(): void {
    // 检查点1：执行上下文清理
    this.registerCleanupCheckpoint(
      'execution-context',
      async (threadId: string) => {
        // 由各执行上下文实现自己的清理
        logger.debug(`Execution context cleanup for thread: ${threadId}`);
      },
      3000
    );
    
    // 检查点2：子工作流清理
    this.registerCleanupCheckpoint(
      'triggered-subworkflow',
      async (threadId: string) => {
        // 由 TriggeredSubworkflowManager 处理
        logger.debug(`Triggered subworkflow cleanup for thread: ${threadId}`);
      },
      5000
    );
  }
  
  /**
   * 订阅生命周期事件
   */
  onLifecycleEvent(
    eventType: ThreadLifecycleEvent['type'],
    handler: (event: ThreadLifecycleEvent) => void | Promise<void>
  ): () => void {
    return this.lifecycleEmitter.on(
      eventType,
      (event: ThreadLifecycleEvent) => {
        try {
          const result = handler(event);
          if (result instanceof Promise) {
            result.catch(err => logger.error(`Event handler error (${eventType}):`, err));
          }
        } catch (error) {
          logger.error(`Event handler error (${eventType}):`, error);
        }
      }
    );
  }
  
  /**
   * 发出生命周期事件
   */
  private emitEvent(event: ThreadLifecycleEvent): void {
    logger.debug(`Lifecycle event: ${event.type} (threadId: ${event.threadId})`);
    this.lifecycleEmitter.emit(event.type, event);
  }
  
  /**
   * 获取线程（只读）
   */
  getThread(threadId: string): ThreadEntity | null {
    return this.threadRegistry.get(threadId);
  }
}
```

#### 使用示例

```typescript
// 注入到 DI 容器
const coordinator = new ThreadLifecycleCoordinator(
  threadBuilder,
  threadRegistry,
  taskRegistry,
  threadExecutor
);

// 注册清理检查点
coordinator.registerCleanupCheckpoint(
  'my-service',
  async (threadId: string) => {
    // 清理我的服务相关数据
    await myService.cleanup(threadId);
  },
  3000
);

// 订阅生命周期事件
coordinator.onLifecycleEvent('CREATED', (event) => {
  console.log(`Thread created: ${event.threadId}`);
});

coordinator.onLifecycleEvent('DELETED', (event) => {
  console.log(`Thread deleted: ${event.threadId}`);
});

// 使用
try {
  // 创建
  const thread = await coordinator.createThread('my-workflow', {
    input: { foo: 'bar' }
  });
  
  // 执行
  const result = await coordinator.executeThread(thread.getThreadId());
  
  // 清理（自动触发所有检查点）
  await coordinator.cleanupThread(thread.getThreadId());
  
} catch (error) {
  console.error('Error:', error);
  // 失败时也会清理
}
```

### 方案2：Graph 浅拷贝实现

#### 改进的 ThreadBuilder

**sdk/core/execution/thread-builder.ts**

```typescript
/**
 * 为线程创建预处理图的浅拷贝
 * 
 * 目的：
 * - 每个线程有独立的元数据副本
 * - 共享不可变的图结构（节点、边）
 * - 减少内存占用
 * - 提供线程隔离
 */
private createGraphCopyForThread(
  preprocessedGraph: PreprocessedGraph
): PreprocessedGraph {
  // 浅拷贝策略：
  // - 图结构（节点、边、邻接表）：共享引用
  // - 元数据（ID映射、配置）：深拷贝
  // - 工作流元数据（触发器、变量）：深拷贝
  
  const graphCopy: PreprocessedGraph = {
    // ============ 共享部分（不可变数据结构）============
    // 节点和边不拷贝，直接共享
    nodes: preprocessedGraph.nodes,
    edges: preprocessedGraph.edges,
    neighbors: preprocessedGraph.neighbors,
    reverseNeighbors: preprocessedGraph.reverseNeighbors,
    startNodeId: preprocessedGraph.startNodeId,
    endNodeIds: preprocessedGraph.endNodeIds,
    
    // ============ 拷贝部分（线程可能修改）============
    // ID 映射（线程专属副本）
    idMapping: {
      ...preprocessedGraph.idMapping,
      nodeIdMap: new Map(preprocessedGraph.idMapping.nodeIdMap),
      edgeIdMap: new Map(preprocessedGraph.idMapping.edgeIdMap),
      subgraphIdMaps: new Map(
        preprocessedGraph.idMapping.subgraphIdMaps?.entries() || []
      )
    },
    
    // 节点配置（线程专属副本）
    nodeConfigs: new Map(preprocessedGraph.nodeConfigs),
    
    // 触发器配置（线程专属副本）
    triggerConfigs: new Map(preprocessedGraph.triggerConfigs),
    
    // 子图关系（拷贝数组）
    subgraphRelationships: [
      ...preprocessedGraph.subgraphRelationships
    ],
    
    // 图分析结果（只读，直接引用）
    graphAnalysis: preprocessedGraph.graphAnalysis,
    
    // 验证结果（只读，直接引用）
    validationResult: preprocessedGraph.validationResult,
    
    // 拓扑排序（拷贝数组）
    topologicalOrder: [
      ...preprocessedGraph.topologicalOrder
    ],
    
    // 子图合并日志（拷贝数组）
    subgraphMergeLogs: [
      ...preprocessedGraph.subgraphMergeLogs
    ],
    
    // 处理时间戳（保持原值）
    processedAt: preprocessedGraph.processedAt,
    
    // ============ 工作流元数据 ============
    workflowId: preprocessedGraph.workflowId,
    workflowVersion: preprocessedGraph.workflowVersion,
    
    // 触发器（拷贝数组）
    triggers: preprocessedGraph.triggers
      ? [...preprocessedGraph.triggers]
      : undefined,
    
    // 变量定义（拷贝数组）
    variables: preprocessedGraph.variables
      ? [...preprocessedGraph.variables]
      : undefined,
    
    // 子工作流标志（直接引用）
    hasSubgraphs: preprocessedGraph.hasSubgraphs,
    
    // 子工作流ID集合（拷贝 Set）
    subworkflowIds: new Set(preprocessedGraph.subworkflowIds),
    
    // 可用工具（拷贝）
    availableTools: preprocessedGraph.availableTools
      ? {
          initial: new Set(preprocessedGraph.availableTools.initial)
        }
      : undefined
  };
  
  return graphCopy;
}

async buildFromPreprocessedGraph(
  preprocessedGraph: PreprocessedGraph,
  options: ThreadOptions = {}
): Promise<ThreadEntity> {
  // ... 验证代码 ...
  
  // 使用浅拷贝而非直接引用
  const threadGraphData = this.createGraphCopyForThread(preprocessedGraph);
  
  const thread: Thread = {
    id: threadId,
    workflowId: preprocessedGraph.workflowId,
    workflowVersion: preprocessedGraph.workflowVersion,
    status: 'CREATED' as ThreadStatus,
    currentNodeId: startNode.id,
    graph: threadGraphData,  // ✅ 线程专属的图副本
    variables: [],
    variableScopes: {
      global: {},
      thread: {},
      local: [],
      loop: []
    },
    input: options.input || {},
    output: {},
    nodeResults: [],
    startTime: now,
    errors: [],
    shouldPause: false,
    shouldStop: false
  };
  
  // ... 其他代码 ...
}
```

#### Graph 引用计数实现

**sdk/core/services/managed-graph-registry.ts**

```typescript
/**
 * ManagedPreprocessedGraph - 带引用计数的图包装器
 */
class ManagedPreprocessedGraph {
  private graph: PreprocessedGraph;
  private refCount: number = 0;
  private threadIds: Set<string> = new Set();
  private createdAt: number = Date.now();
  private lastAccessedAt: number = Date.now();
  
  constructor(graph: PreprocessedGraph) {
    this.graph = graph;
  }
  
  addRef(threadId: string): PreprocessedGraph {
    if (!this.threadIds.has(threadId)) {
      this.threadIds.add(threadId);
      this.refCount++;
      logger.debug(`Graph ref added: ${this.graph.workflowId} (thread: ${threadId}, refCount: ${this.refCount})`);
    }
    this.lastAccessedAt = Date.now();
    return this.graph;
  }
  
  removeRef(threadId: string): boolean {
    if (this.threadIds.has(threadId)) {
      this.threadIds.delete(threadId);
      this.refCount--;
      logger.debug(`Graph ref removed: ${this.graph.workflowId} (thread: ${threadId}, refCount: ${this.refCount})`);
    }
    return this.refCount === 0;
  }
  
  canDelete(): boolean {
    return this.refCount === 0;
  }
  
  getRefCount(): number {
    return this.refCount;
  }
  
  getThreadIds(): string[] {
    return Array.from(this.threadIds);
  }
  
  getGraph(): PreprocessedGraph {
    return this.graph;
  }
  
  getMetrics() {
    return {
      workflowId: this.graph.workflowId,
      refCount: this.refCount,
      threadIds: this.getThreadIds(),
      createdAt: this.createdAt,
      lastAccessedAt: this.lastAccessedAt,
      age: Date.now() - this.createdAt
    };
  }
}

/**
 * 增强的 GraphRegistry，支持引用计数
 */
export class ManagedGraphRegistry {
  private graphs: Map<string, ManagedPreprocessedGraph> = new Map();
  private logger = createContextualLogger();
  
  /**
   * 注册图（持有强所有权）
   */
  register(workflowId: string, graph: PreprocessedGraph): void {
    const existing = this.graphs.get(workflowId);
    if (existing) {
      this.logger.warn(`Graph already registered: ${workflowId}, overwriting`);
    }
    
    this.graphs.set(workflowId, new ManagedPreprocessedGraph(graph));
    this.logger.debug(`Graph registered: ${workflowId}`);
  }
  
  /**
   * 获取图并增加引用计数
   */
  getAndRef(workflowId: string, threadId: string): PreprocessedGraph | null {
    const managed = this.graphs.get(workflowId);
    if (!managed) {
      return null;
    }
    
    return managed.addRef(threadId);
  }
  
  /**
   * 获取图（不增加引用计数）
   * 仅用于只读访问
   */
  get(workflowId: string): PreprocessedGraph | null {
    const managed = this.graphs.get(workflowId);
    return managed ? managed.getGraph() : null;
  }
  
  /**
   * 释放引用
   */
  releaseRef(workflowId: string, threadId: string): boolean {
    const managed = this.graphs.get(workflowId);
    if (!managed) {
      this.logger.warn(`Graph not found for release: ${workflowId}`);
      return false;
    }
    
    const canDelete = managed.removeRef(threadId);
    if (canDelete) {
      this.graphs.delete(workflowId);
      this.logger.debug(`Graph deleted: ${workflowId} (ref count reached 0)`);
      return true;
    }
    
    return false;
  }
  
  /**
   * 强制删除图（即使有活跃引用）
   * ⚠️ 谨慎使用，可能导致已有线程无法访问图
   */
  forceDelete(workflowId: string): boolean {
    const managed = this.graphs.get(workflowId);
    if (!managed) {
      return false;
    }
    
    const refCount = managed.getRefCount();
    if (refCount > 0) {
      this.logger.warn(
        `Force deleting graph with active refs: ${workflowId} (refCount: ${refCount})`
      );
    }
    
    this.graphs.delete(workflowId);
    return true;
  }
  
  /**
   * 获取图的统计信息
   */
  getStats(workflowId: string) {
    const managed = this.graphs.get(workflowId);
    return managed ? managed.getMetrics() : null;
  }
  
  /**
   * 获取所有图的统计信息
   */
  getAllStats() {
    return Array.from(this.graphs.values()).map(m => m.getMetrics());
  }
  
  /**
   * 清理泄漏的引用（定期调用）
   * 
   * @param maxAge 最大年龄（毫秒），超过这个时间且无活跃引用的图将被删除
   * @returns 清理的图数量
   */
  cleanup(maxAge: number = 24 * 60 * 60 * 1000): number {
    const now = Date.now();
    let cleanedCount = 0;
    
    for (const [workflowId, managed] of this.graphs.entries()) {
      const metrics = managed.getMetrics();
      const age = metrics.age;
      const lastAccessed = now - metrics.lastAccessedAt;
      
      // 条件：没有活跃引用 且 已经很老
      if (managed.canDelete() && age > maxAge) {
        this.graphs.delete(workflowId);
        cleanedCount++;
        this.logger.debug(`Cleaned up old graph: ${workflowId} (age: ${age}ms)`);
      }
    }
    
    return cleanedCount;
  }
}
```

---

## 第三部分：集成示例

### 完整的改进流程

```typescript
// 1. 初始化协调器
const lifecycleCoordinator = new ThreadLifecycleCoordinator(
  threadBuilder,
  threadRegistry,
  taskRegistry,
  threadExecutor
);

// 2. 注册清理检查点
lifecycleCoordinator.registerCleanupCheckpoint(
  'triggered-subworkflow',
  async (threadId: string) => {
    // TriggeredSubworkflowManager 监听并清理
    await triggeredSubworkflowManager.cleanup(threadId);
  },
  5000
);

// 3. 创建线程（自动注册）
const thread = await lifecycleCoordinator.createThread('workflow-1', {
  input: { /* ... */ }
});

// 4. 执行线程
try {
  const result = await lifecycleCoordinator.executeThread(thread.getThreadId());
  console.log('Execution result:', result);
} catch (error) {
  console.error('Execution failed:', error);
}

// 5. 自动清理（触发所有检查点）
await lifecycleCoordinator.cleanupThread(thread.getThreadId());

// 结果：
// - 所有清理检查点都被执行
// - ThreadRegistry 中删除了线程
// - TaskRegistry 中清理了相关任务
// - TriggeredSubworkflowManager 中清理了子工作流
// - 所有相关资源都被正确释放
```

---

## 第四部分：迁移检查清单

### ThreadRegistry 的修改

- [ ] 添加所有权追踪字段
- [ ] 实现 `register(entity, owner)` 方法
- [ ] 实现 `delete(threadId, owner)` 方法（返回 boolean）
- [ ] 添加防护：delete 前检查其他所有者
- [ ] 添加日志记录所有权变化

### TriggeredSubworkflowManager 的修改

- [ ] 移除直接的清理逻辑
- [ ] 注册清理检查点到 ThreadLifecycleCoordinator
- [ ] 实现 `async cleanup(threadId)` 方法
- [ ] 更新 `handleSubgraphCompleted/Failed` 触发事件而非直接清理

### ThreadBuilder 的修改

- [ ] 实现 `createGraphCopyForThread()` 方法
- [ ] 修改 `buildFromPreprocessedGraph()` 使用浅拷贝
- [ ] 添加图版本检查（可选）

### GraphRegistry 的替换

- [ ] 创建 `ManagedGraphRegistry` 类
- [ ] 迁移所有 `get()` 调用到 `getAndRef()` 或 `releaseRef()`
- [ ] 添加定期清理任务
- [ ] 添加监控和告警

### 测试添加

- [ ] 单元测试：所有权冲突检测
- [ ] 集成测试：完整的生命周期流程
- [ ] 内存测试：引用计数的正确性
- [ ] 并发测试：多线程的清理顺序

