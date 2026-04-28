# Thread 执行实例

本文档详细描述 Thread 作为 Workflow 执行实例的结构、创建流程和生命周期管理。

## 概述

Thread 是 Workflow 的一次执行实例，包含运行时数据和状态。ThreadEntity 是 Thread 的实体封装，提供数据访问接口和状态管理。

## 核心类型

### Thread

**位置**：`packages/types/src/thread/definition.ts`

```typescript
interface Thread {
  // 基本标识
  id: ID;                                    // Thread 唯一标识符
  workflowId: ID;                            // 关联的 Workflow ID
  workflowVersion: Version;                  // Workflow 版本
  
  // 执行状态
  currentNodeId: ID;                         // 当前执行节点 ID
  
  // 图结构引用
  graph: PreprocessedGraph;                  // 预处理图（引用）
  
  // 变量管理
  variables: ThreadVariable[];               // 变量数组（用于持久化）
  variableScopes: VariableScopes;            // 四层作用域变量存储
  
  // 输入输出
  input: Record<string, unknown>;            // 输入数据
  output: Record<string, unknown>;           // 输出数据
  
  // 执行历史
  nodeResults: NodeExecutionResult[];        // 节点执行结果
  errors: unknown[];                         // 错误信息
  
  // 上下文数据
  contextData?: Record<string, unknown>;     // 上下文数据
  
  // Thread 类型和关系
  threadType?: ThreadType;                   // Thread 类型
  forkJoinContext?: ForkJoinContext;         // Fork/Join 上下文
  triggeredSubworkflowContext?: TriggeredSubworkflowContext;  // 触发子工作流上下文
}
```

### ThreadType

```typescript
enum ThreadType {
  MAIN = "MAIN",                           // 主 Thread
  FORK_JOIN = "FORK_JOIN",                 // Fork/Join 子 Thread
  TRIGGERED_SUBWORKFLOW = "TRIGGERED_SUBWORKFLOW"  // 触发子工作流
}
```

### ThreadStatus

**位置**：`packages/types/src/thread/status.ts`

```typescript
enum ThreadStatus {
  CREATED = "CREATED",       // 已创建
  RUNNING = "RUNNING",       // 运行中
  PAUSED = "PAUSED",         // 已暂停
  COMPLETED = "COMPLETED",   // 已完成
  FAILED = "FAILED",         // 失败
  CANCELLED = "CANCELLED",   // 已取消
  TIMEOUT = "TIMEOUT"        // 超时
}
```

### VariableScopes

**位置**：`packages/types/src/thread/scopes.ts`

```typescript
interface VariableScopes {
  global: Record<string, unknown>;      // 全局作用域
  thread: Record<string, unknown>;      // Thread 作用域
  local: Record<string, unknown>[];     // 局部作用域栈
  loop: Record<string, unknown>[];      // 循环作用域栈
}
```

### NodeExecutionResult

**位置**：`packages/types/src/thread/history.ts`

```typescript
interface NodeExecutionResult {
  nodeId: ID;                           // 节点 ID
  nodeType: NodeType;                   // 节点类型
  status: ExecutionStatus;              // 执行状态
  step: number;                         // 执行步数
  startTime: Timestamp;                 // 开始时间
  endTime: Timestamp;                   // 结束时间
  executionTime: number;                // 执行时长
  error?: unknown;                      // 错误信息
  output?: unknown;                     // 输出数据
}
```

---

## ThreadEntity

### 职责

ThreadEntity 是 Thread 的实体封装，提供：
- 数据访问接口（getter/setter）
- 运行时状态管理
- 多个状态管理器的持有

**位置**：`sdk/graph/entities/thread-entity.ts`

### 核心结构

```typescript
class ThreadEntity {
  readonly id: string;                           // Thread ID
  
  private readonly thread: Thread;               // Thread 数据对象
  readonly state: ThreadState;                   // 运行时状态
  private readonly executionState: ExecutionState;  // 子图执行栈
  readonly messageHistoryManager: MessageHistory;   // 消息历史
  readonly variableStateManager: VariableState;     // 变量状态
  
  abortController?: AbortController;             // 停止控制器
  conversationManager?: ConversationSession;     // 对话会话
  triggerManager?: unknown;                      // 触发器管理
  toolVisibilityCoordinator?: unknown;           // 工具可见性协调
  
  private childAgentLoopId: Set<string>;         // 子 AgentLoop ID
}
```

### 主要方法

#### 基本属性访问

```typescript
getWorkflowId(): string {
  return this.thread.workflowId;
}

getStatus(): ThreadStatus {
  return this.state.status;
}

setStatus(status: ThreadStatus): void {
  this.state.status = status;
}

getCurrentNodeId(): string {
  return this.thread.currentNodeId;
}

setCurrentNodeId(nodeId: string): void {
  this.thread.currentNodeId = nodeId;
}

getInput(): Record<string, unknown> {
  return this.thread.input;
}

getOutput(): Record<string, unknown> {
  return this.thread.output;
}

setOutput(output: Record<string, unknown>): void {
  this.thread.output = output;
}
```

#### 执行结果管理

```typescript
addNodeResult(result: NodeExecutionResult): void {
  this.thread.nodeResults.push(result);
}

getNodeResults(): NodeExecutionResult[] {
  return this.thread.nodeResults;
}

getErrors(): unknown[] {
  return this.thread.errors;
}
```

#### 变量管理

```typescript
getVariable(name: string): unknown {
  return this.variableStateManager.getVariable(name);
}

setVariable(name: string, value: unknown): void {
  this.variableStateManager.setVariable(name, value);
}

getAllVariables(): Record<string, unknown> {
  return this.variableStateManager.getAllVariables();
}

deleteVariable(name: string): boolean {
  return this.variableStateManager.deleteVariable(name);
}
```

#### 消息管理

```typescript
addMessage(message: LLMMessage): void {
  this.messageHistoryManager.addMessage(message);
  if (this.conversationManager) {
    this.conversationManager.addMessage(message);
  }
}

getMessages(): LLMMessage[] {
  return this.messageHistoryManager.getMessages();
}

getRecentMessages(count: number): LLMMessage[] {
  return this.messageHistoryManager.getRecentMessages(count);
}
```

#### 中断控制

```typescript
pause(): void {
  this.state.pause();
}

resume(): void {
  this.state.resume();
}

stop(): void {
  this.state.cancel();
  this.abort();
}

shouldPause(): boolean {
  return this.state.shouldPause();
}

shouldStop(): boolean {
  return this.state.shouldStop();
}

interrupt(type: "PAUSE" | "STOP"): void {
  this.state.interrupt(type);
  if (type === "STOP") {
    this.abort();
  }
}
```

#### 子图执行栈

```typescript
enterSubgraph(workflowId: ID, parentWorkflowId: ID, input: unknown): void {
  this.executionState.enterSubgraph(workflowId, parentWorkflowId, input);
}

exitSubgraph(): void {
  this.executionState.exitSubgraph();
}

getCurrentSubgraphContext(): SubgraphContext | null {
  return this.executionState.getCurrentSubgraphContext();
}

getSubgraphStack(): SubgraphContext[] {
  return this.executionState.getSubgraphStack();
}
```

#### 子 Thread 管理

```typescript
registerChildThread(childThreadId: ID): void {
  if (!this.thread.triggeredSubworkflowContext) {
    this.thread.triggeredSubworkflowContext = {
      parentThreadId: "",
      childThreadIds: [],
      triggeredSubworkflowId: "",
    };
  }
  if (!this.thread.triggeredSubworkflowContext.childThreadIds.includes(childThreadId)) {
    this.thread.triggeredSubworkflowContext.childThreadIds.push(childThreadId);
  }
}

getChildThreadIds(): ID[] {
  return this.thread.triggeredSubworkflowContext?.childThreadIds || [];
}
```

---

## Thread 创建流程

### ThreadBuilder

**位置**：`sdk/graph/execution/factories/thread-builder.ts`

```typescript
class ThreadBuilder {
  async build(workflowId: string, options: ThreadOptions = {}): Promise<ThreadEntity> {
    // 1. 从 GraphRegistry 获取预处理图
    const preprocessedGraph = this.getGraphRegistry().get(workflowId);
    
    if (!preprocessedGraph) {
      throw new ExecutionError(`Workflow '${workflowId}' not found or not preprocessed`);
    }
    
    // 2. 从预处理图构建 ThreadEntity
    const threadEntity = await this.buildFromPreprocessedGraph(preprocessedGraph, options);
    
    return threadEntity;
  }
  
  private async buildFromPreprocessedGraph(
    preprocessedGraph: PreprocessedGraph,
    options: ThreadOptions = {}
  ): Promise<ThreadEntity> {
    // 1. 验证预处理图
    if (!preprocessedGraph.nodes || preprocessedGraph.nodes.size === 0) {
      throw new RuntimeValidationError("Preprocessed graph must have at least one node");
    }
    
    const startNode = Array.from(preprocessedGraph.nodes.values()).find(n => n.type === "START");
    if (!startNode) {
      throw new RuntimeValidationError("Preprocessed graph must have a START node");
    }
    
    // 2. 创建 Thread 对象
    const threadId = generateId();
    const thread: Thread = {
      id: threadId,
      workflowId: preprocessedGraph.workflowId,
      workflowVersion: preprocessedGraph.workflowVersion,
      currentNodeId: startNode.id,
      graph: preprocessedGraph,
      variables: [],
      variableScopes: {
        global: {},
        thread: {},
        local: [],
        loop: [],
      },
      input: options.input || {},
      output: {},
      nodeResults: [],
      errors: [],
      threadType: "MAIN",
    };
    
    // 3. 初始化变量
    const variableCoordinator = this.getVariableCoordinator();
    variableCoordinator.initializeFromWorkflow(preprocessedGraph.variables || []);
    
    // 4. 创建 ExecutionState
    const executionState = new ExecutionState();
    
    // 5. 创建 ConversationSession
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      threadId: thread.id,
      workflowId: preprocessedGraph.workflowId,
    });
    
    // 6. 创建 ThreadEntity
    const threadEntity = new ThreadEntity(thread, executionState, undefined, conversationManager);
    
    return threadEntity;
  }
}
```

### 创建流程图

```
ThreadBuilder.build(workflowId, options)
            ↓
GraphRegistry.get(workflowId)
            ↓
PreprocessedGraph
            ↓
    [验证预处理图]
            ├─ 检查节点存在
            ├─ 检查 START 节点
            └─ 检查 END 节点
            ↓
    [创建 Thread 对象]
            ├─ 生成 threadId
            ├─ 设置 workflowId
            ├─ 设置 currentNodeId = startNode.id
            ├─ 引用 PreprocessedGraph
            ├─ 初始化 variableScopes
            └─ 设置 input
            ↓
    [初始化组件]
            ├─ VariableCoordinator.initializeFromWorkflow()
            ├─ new ExecutionState()
            └─ new ConversationSession()
            ↓
new ThreadEntity(thread, executionState, ...)
            ↓
ThreadRegistry.register(threadEntity)
```

---

## Thread 执行流程

### ThreadExecutor

**位置**：`sdk/graph/execution/executors/thread-executor.ts`

```typescript
class ThreadExecutor {
  async executeThread(threadEntity: ThreadEntity): Promise<ThreadResult> {
    const threadId = threadEntity.id;
    const workflowId = threadEntity.getWorkflowId();
    
    // 1. 验证 workflow 图存在
    const preprocessedGraph = this.graphRegistry.get(workflowId);
    if (!preprocessedGraph) {
      throw new Error(`Graph not found for workflow: ${workflowId}`);
    }
    
    // 2. 创建 ThreadExecutionCoordinator
    const threadExecutionCoordinator = this.threadExecutionCoordinatorFactory.create(threadEntity);
    
    // 3. 执行 Thread
    const result = await threadExecutionCoordinator.execute();
    
    return result;
  }
}
```

### ThreadExecutionCoordinator

**位置**：`sdk/graph/execution/coordinators/thread-execution-coordinator.ts`

```typescript
class ThreadExecutionCoordinator {
  async execute(): Promise<ThreadResult> {
    const threadId = this.threadEntity.id;
    const startTime = this.threadEntity.getStartTime();
    
    // 执行循环
    while (true) {
      // 1. 检查中断状态
      if (this.interruptionManager.shouldPause()) {
        throw new ThreadInterruptedException("Thread execution paused", "PAUSE", ...);
      }
      
      if (this.interruptionManager.shouldStop()) {
        throw new ThreadInterruptedException("Thread execution stopped", "STOP", ...);
      }
      
      // 2. 获取当前节点
      const currentNodeId = this.threadEntity.getCurrentNodeId();
      if (!currentNodeId) break;
      
      const graphNode = this.navigator.getGraph().getNode(currentNodeId);
      if (!graphNode) break;
      
      // 3. 执行节点
      const result = await this.nodeExecutionCoordinator.executeNode(
        this.threadEntity,
        currentNode
      );
      
      // 4. 记录结果
      this.threadEntity.addNodeResult(result);
      
      // 5. 移动到下一节点
      if (result.status === "COMPLETED") {
        const nextNode = this.navigator.getNextNode(currentNodeId);
        if (nextNode && nextNode.nextNodeId) {
          this.threadEntity.setCurrentNodeId(nextNode.nextNodeId);
        } else {
          break;
        }
      } else {
        break;
      }
    }
    
    // 6. 构建执行结果
    const endTime = this.threadEntity.getEndTime() || Date.now();
    const executionTime = endTime - (startTime || Date.now());
    
    return {
      threadId,
      output: this.threadEntity.getOutput(),
      executionTime,
      nodeResults: this.threadEntity.getNodeResults(),
      metadata: {
        status: this.threadEntity.getStatus(),
        startTime: startTime || Date.now(),
        endTime,
        executionTime,
        nodeCount: this.threadEntity.getNodeResults().length,
        errorCount: this.threadEntity.getErrors().length,
      },
    };
  }
}
```

### 执行流程图

```
ThreadExecutor.executeThread(threadEntity)
            ↓
ThreadExecutionCoordinator.execute()
            ↓
    [执行循环]
            ├─ 检查中断状态
            │   ├─ shouldPause() → 抛出中断异常
            │   └─ shouldStop() → 抛出中断异常
            ├─ 获取当前节点
            │   └─ currentNodeId = threadEntity.getCurrentNodeId()
            ├─ NodeExecutionCoordinator.executeNode()
            │   ├─ 触发 NODE_STARTED 事件
            │   ├─ 创建检查点（可选）
            │   ├─ 执行 BEFORE_EXECUTE Hook
            │   ├─ 执行节点逻辑
            │   ├─ 执行 AFTER_EXECUTE Hook
            │   ├─ 创建检查点（可选）
            │   └─ 触发 NODE_COMPLETED 事件
            ├─ 记录执行结果
            │   └─ threadEntity.addNodeResult(result)
            └─ 移动到下一节点
                └─ threadEntity.setCurrentNodeId(nextNodeId)
            ↓
    [构建执行结果]
            └─ ThreadResult
```

---

## Thread 生命周期管理

### ThreadLifecycleCoordinator

**位置**：`sdk/graph/execution/coordinators/thread-lifecycle-coordinator.ts`

```typescript
class ThreadLifecycleCoordinator {
  // 执行 Thread
  async execute(workflowId: string, options: ThreadOptions = {}): Promise<ThreadResult> {
    // Step 1: 构建 ThreadEntity
    const threadEntity = await this.threadBuilder.build(workflowId, options);
    
    // Step 2: 注册 ThreadEntity
    this.threadRegistry.register(threadEntity);
    
    // Step 3: 启动 Thread
    await this.threadStateTransitor.startThread(threadEntity);
    
    // Step 4: 执行 Thread
    const result = await this.threadExecutor.executeThread(threadEntity);
    
    // Step 5: 根据执行结果更新状态
    if (result.metadata?.status === "COMPLETED") {
      await this.threadStateTransitor.completeThread(threadEntity, result);
    } else {
      await this.threadStateTransitor.failThread(threadEntity, error);
    }
    
    return result;
  }
  
  // 暂停 Thread
  async pauseThread(threadId: string): Promise<void> {
    const threadEntity = this.threadRegistry.get(threadId);
    
    // 1. 请求暂停
    threadEntity.interrupt("PAUSE");
    
    // 2. 委托状态转换
    await this.threadStateTransitor.pauseThread(threadEntity);
  }
  
  // 恢复 Thread
  async resumeThread(threadId: string): Promise<ThreadResult> {
    const threadEntity = this.threadRegistry.get(threadId);
    
    // 1. 委托状态转换
    await this.threadStateTransitor.resumeThread(threadEntity);
    
    // 2. 重置中断状态
    threadEntity.resetInterrupt();
    
    // 3. 继续执行
    return await this.threadExecutor.executeThread(threadEntity);
  }
  
  // 停止 Thread
  async stopThread(threadId: string): Promise<void> {
    const threadEntity = this.threadRegistry.get(threadId);
    
    // 1. 请求停止
    threadEntity.interrupt("STOP");
    
    // 2. 委托状态转换
    await this.threadStateTransitor.cancelThread(threadEntity, "user_requested");
    
    // 3. 级联取消子 Thread
    await this.threadStateTransitor.cascadeCancel(threadId);
    
    // 4. 清理子 AgentLoop
    await this.cleanupChildAgentLoops(threadId);
  }
}
```

### ThreadStateTransitor

**位置**：`sdk/graph/execution/coordinators/thread-state-transitor.ts`

负责原子状态转换：

```typescript
class ThreadStateTransitor {
  // 启动 Thread
  async startThread(threadEntity: ThreadEntity): Promise<void> {
    validateTransition(threadEntity.id, previousStatus, "RUNNING");
    threadEntity.setStatus("RUNNING");
    await emit(this.eventManager, buildThreadStartedEvent(threadEntity));
    await emit(this.eventManager, buildThreadStateChangedEvent(...));
  }
  
  // 暂停 Thread
  async pauseThread(threadEntity: ThreadEntity): Promise<void> {
    validateTransition(threadEntity.id, currentStatus, "PAUSED");
    threadEntity.setStatus("PAUSED");
    await emit(this.eventManager, buildThreadPausedEvent(threadEntity));
  }
  
  // 恢复 Thread
  async resumeThread(threadEntity: ThreadEntity): Promise<void> {
    validateTransition(threadEntity.id, currentStatus, "RUNNING");
    threadEntity.setStatus("RUNNING");
    await emit(this.eventManager, buildThreadResumedEvent(threadEntity));
  }
  
  // 完成 Thread
  async completeThread(threadEntity: ThreadEntity, result: ThreadResult): Promise<void> {
    validateTransition(threadEntity.id, previousStatus, "COMPLETED");
    threadEntity.setStatus("COMPLETED");
    threadEntity.state.complete();
    this.graphConversationSession.cleanup();
    await emit(this.eventManager, buildThreadCompletedEvent(threadEntity, result));
  }
  
  // 失败 Thread
  async failThread(threadEntity: ThreadEntity, error: Error): Promise<void> {
    validateTransition(threadEntity.id, previousStatus, "FAILED");
    threadEntity.setStatus("FAILED");
    threadEntity.state.fail(error);
    await emit(this.eventManager, buildThreadFailedEvent(...));
  }
  
  // 取消 Thread
  async cancelThread(threadEntity: ThreadEntity, reason?: string): Promise<void> {
    validateTransition(threadEntity.id, currentStatus, "CANCELLED");
    threadEntity.setStatus("CANCELLED");
    threadEntity.state.cancel();
    await emit(this.eventManager, buildThreadCancelledEvent(threadEntity, reason));
  }
}
```

### 状态转换图

```
         ┌─────────┐
         │ CREATED │
         └────┬────┘
              │ startThread()
              ↓
         ┌─────────┐
    ┌───→│ RUNNING │←───┐
    │    └────┬────┘    │
    │         │         │
    │    pauseThread()  │ resumeThread()
    │         │         │
    │         ↓         │
    │    ┌─────────┐    │
    └────┤ PAUSED  ├────┘
         └────┬────┘
              │
              │ stopThread()
              ↓
         ┌───────────┐
         │ CANCELLED │
         └───────────┘
              
         ┌───────────┐
         │ COMPLETED │  (正常完成)
         └───────────┘
              
         ┌───────────┐
         │  FAILED   │  (执行失败)
         └───────────┘
```

---

## ThreadRegistry

### 职责

ThreadRegistry 负责 ThreadEntity 的存储和查询。

**位置**：`sdk/graph/stores/thread-registry.ts`

```typescript
class ThreadRegistry {
  private threadEntities: Map<string, ThreadEntity> = new Map();
  
  // 注册
  register(threadEntity: ThreadEntity): void {
    this.threadEntities.set(threadEntity.id, threadEntity);
  }
  
  // 获取
  get(threadId: string): ThreadEntity | null {
    return this.threadEntities.get(threadId) || null;
  }
  
  // 删除
  delete(threadId: string): void {
    this.threadEntities.delete(threadId);
  }
  
  // 获取所有
  getAll(): ThreadEntity[] {
    return Array.from(this.threadEntities.values());
  }
  
  // 按状态查询
  getByStatus(status: ThreadStatus): ThreadEntity[] {
    return this.getAll().filter(entity => entity.getStatus() === status);
  }
  
  // 获取运行中的
  getRunning(): ThreadEntity[] {
    return this.getByStatus("RUNNING");
  }
  
  // 获取已暂停的
  getPaused(): ThreadEntity[] {
    return this.getByStatus("PAUSED");
  }
  
  // 清理已完成的
  cleanupCompleted(): number {
    const completedIds = this.getCompleted().map(e => e.id);
    for (const id of completedIds) {
      this.delete(id);
    }
    return completedIds.length;
  }
}
```

---

## 设计原则

### 1. 数据与行为分离

- Thread：纯数据对象
- ThreadEntity：数据封装和访问
- ThreadExecutor：执行逻辑
- ThreadLifecycleCoordinator：生命周期管理

### 2. 状态封装

- ThreadEntity 持有多个状态管理器
- 通过 getter/setter 访问数据
- 状态管理器分离关注点

### 3. 无状态执行器

- ThreadExecutor 不持有状态
- 所有状态通过 ThreadEntity 传递
- 便于测试和并发

### 4. 事件驱动

- 状态转换触发事件
- 组件间通过事件通信
- 支持外部监听

---

## 相关文档

- [整体数据流](./README.md)
- [Workflow 定义与管理](./workflow-definition.md)
- [Graph 预处理](./graph-preprocessing.md)
