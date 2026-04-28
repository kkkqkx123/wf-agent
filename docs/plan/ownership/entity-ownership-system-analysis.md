# 实体所有权体系分析

## 文档概述

本文档分析 Modular Agent Framework SDK 中各核心实体的持有关系、当前的所有权体系问题，以及推荐的改进方案。

## 第一部分：核心实体与持有关系

### 1. 实体层级结构

```
PreprocessedGraph (图结构)
    ↓ held by
GraphRegistry (全局单例)
    ↓
Thread (纯数据对象)
    ↓ owned by
ThreadEntity (实体包装器)
    ↓ referenced by
ThreadRegistry (全局单例注册表)
    ↑
    ← also referenced by
TaskRegistry (全局单例任务表)
    ↑
    ← also referenced by
TriggeredSubworkflowManager (全局单例服务)
```

### 2. 实体定义与职责

#### 2.1 PreprocessedGraph（预处理图结构）

**定义** (packages/types/src/graph/preprocessed-graph.ts)：
- 扩展 Graph 接口的综合图表示
- 包含节点、边、邻接表等核心图结构
- 添加预处理元数据：ID映射、拓扑排序、子图关系等

**关键字段**：
```typescript
interface PreprocessedGraph extends Graph {
  // 核心图结构
  nodes: Map<ID, GraphNode>;          // 节点映射，O(1)查询
  edges: Map<ID, GraphEdge>;          // 边映射，O(1)查询
  neighbors: Map<ID, Set<ID>>;        // 邻接表（前向）
  reverseNeighbors: Map<ID, Set<ID>>; // 邻接表（反向）
  
  // 预处理元数据（不可变）
  idMapping: IdMapping;               // ID转换映射
  topologicalOrder: ID[];             // 拓扑排序
  graphAnalysis: GraphAnalysisResult; // 循环、可达性等分析
  subgraphMergeLogs: SubgraphMergeLog[]; // 合并历史
  
  // 工作流元数据
  workflowId: ID;
  workflowVersion: Version;
  triggers?: WorkflowTrigger[];
  variables?: WorkflowVariable[];
  availableTools?: { initial: Set<string> };
}
```

**性质**：
- **大小**：O(N + E)，其中N为节点数，E为边数
- **不可变性**：构建后不变，用于多个线程
- **共享性**：单一 PreprocessedGraph 被多个 Thread 引用

#### 2.2 Thread（线程纯数据对象）

**定义** (packages/types/src/thread/definition.ts)：
```typescript
interface Thread {
  id: ID;                              // 唯一标识
  workflowId: ID;
  workflowVersion: Version;
  status: ThreadStatus;
  currentNodeId: ID;
  
  graph: PreprocessedGraph;            // ⚠️ 直接引用共享图结构
  
  variables: ThreadVariable[];
  variableScopes: VariableScopes;      // 4级作用域
  input: Record<string, any>;
  output: Record<string, any>;
  nodeResults: NodeExecutionResult[];
  
  startTime: Timestamp;
  endTime?: Timestamp;
  errors: any[];
  contextData?: Record<string, any>;
  
  // 线程关系管理
  threadType?: ThreadType;             // MAIN | FORK_JOIN | TRIGGERED
  forkJoinContext?: ForkJoinContext;   // Fork/Join 上下文
  triggeredSubworkflowContext?: TriggeredSubworkflowContext; // 触发子工作流
}
```

**职责**：
- 纯数据容器，无方法
- 持有对 PreprocessedGraph 的引用
- 持有线程的执行状态、变量、结果等

**关键问题**：
- Thread.graph 持有对共享 PreprocessedGraph 的直接引用
- 多个 Thread 可能引用同一个 PreprocessedGraph（效率考虑）
- 当 PreprocessedGraph 被修改或删除时，可能出现数据不一致

#### 2.3 ThreadEntity（实体包装器）

**定义** (sdk/core/entities/thread-entity.ts)：
```typescript
export class ThreadEntity {
  readonly thread: Thread;              // 暴露底层数据对象
  readonly id: string;
  
  messages: LLMMessage[] = [];
  triggerManager?: any;
  private variables: Map<string, any>;
  abortController?: AbortController;
  conversationManager?: ConversationManager;
  
  constructor(
    thread: Thread,
    private readonly executionState: ExecutionState,
    conversationManager?: ConversationManager
  ) { }
}
```

**职责**：
- 包装 Thread 数据对象，提供方法
- 持有 ExecutionState（执行栈）
- 持有 ConversationManager（对话历史）
- 持有 AbortController（中止控制）

**所有权模式**：
- 强所有权：拥有 Thread 实例的完整生命周期
- 强所有权：管理 ExecutionState 的创建和销毁

#### 2.4 GraphRegistry（图注册表 - 全局单例）

**定义** (sdk/core/services/graph-registry.ts)：
```typescript
export class GraphRegistry {
  private preprocessedGraphs: Map<string, PreprocessedGraph>;
  
  register(workflowId: string, graph: PreprocessedGraph): void
  get(workflowId: string): PreprocessedGraph | null
  delete(workflowId: string): void
}
```

**职责**：
- 中央仓库，管理所有已预处理的图
- 将 workflowId 映射到 PreprocessedGraph
- 支持图的注册、查询和删除

**所有权模式**：
- 强所有权：管理 PreprocessedGraph 的生命周期
- 强所有权：决定何时删除图

#### 2.5 ThreadRegistry（线程注册表 - 全局单例）

**定义** (sdk/core/services/thread-registry.ts)：
```typescript
export class ThreadRegistry {
  private threadEntities: Map<string, ThreadEntity> = new Map();
  
  register(threadEntity: ThreadEntity): void
  get(threadId: string): ThreadEntity | null
  delete(threadId: string): void
  getAll(): ThreadEntity[]
  isWorkflowActive(workflowId: string): boolean
}
```

**职责**：
- 线程中央注册表
- 跟踪所有活跃的 ThreadEntity 实例
- 提供线程的快速查询和存活检查

**所有权模式**：
- 引用所有权：持有对 ThreadEntity 的引用
- 不管理生命周期：ThreadEntity 的创建由 ThreadBuilder 负责，删除由调用方决定

#### 2.6 TaskRegistry（任务注册表 - 全局单例）

**定义** (sdk/core/services/task-registry.ts)：
```typescript
export interface TaskInfo {
  id: string;
  threadEntity: ThreadEntity;          // ⚠️ 持有对 ThreadEntity 的引用
  status: TaskStatus;
  submitTime: number;
  timeout?: number;
  startTime?: number;
  completeTime?: number;
  result?: ThreadResult;
  error?: Error;
}

export class TaskRegistry {
  private tasks: Map<string, TaskInfo>;
  
  register(threadEntity: ThreadEntity, manager: TaskManager, timeout?: number): string
  delete(taskId: string): boolean
  cleanup(retentionTime?: number): number
}
```

**职责**：
- 任务中央注册表
- 跟踪所有任务的状态、时间、结果等
- 支持任务的路由和清理

**所有权模式**：
- 引用所有权：持有对 ThreadEntity 的引用
- 不管理生命周期：但会在任务完成后删除任务记录

#### 2.7 TriggeredSubworkflowManager（触发子工作流管理器 - 全局单例）

**定义** (sdk/core/services/triggered-subworkflow-manager.ts)：
```typescript
export class TriggeredSubworkflowManager implements TaskManager {
  private threadRegistry: ThreadRegistry;
  private taskRegistry: TaskRegistry;
  private activeTasks: Map<string, {
    taskId: string;
    threadId: string;
    submitTime: number;
    timeout: number;
  }> = new Map();
  
  async executeTriggeredSubgraph(task: TriggeredSubgraphTask): Promise<...>
  private unregisterParentChildRelationship(subgraphEntity: ThreadEntity): void
  async handleSubgraphCompleted(threadId: string, result: ExecutedSubgraphResult): void
  async handleSubgraphFailed(threadId: string, error: Error): void
}
```

**职责**：
- 管理触发子工作流的完整生命周期
- 协调任务队列和线程池
- 管理子工作流的创建、执行、完成
- 处理父子线程关系

**所有权模式**：
- 引用所有权：通过 ThreadRegistry 和 TaskRegistry 获取 ThreadEntity
- 不直接持有：但在处理完成后会访问和修改
- 生命周期参与：决定何时删除父子关系和清理任务

---

## 第二部分：当前所有权体系问题分析

### 问题1：多层引用导致的生命周期冲突

**场景描述**：
```
ThreadBuilder 创建 ThreadEntity
    ↓
ThreadRegistry.register(threadEntity)  [持有引用1]
    ↓
TaskRegistry.register(threadEntity)    [持有引用2]
    ↓
TriggeredSubworkflowManager 访问
    ↓
ThreadRegistry.delete(threadId)        [删除引用1]
    ↓
TaskRegistry 和 TriggeredSubworkflowManager 仍持有引用2
    ↓
后续访问可能出现数据不一致
```

**具体代码问题** (triggered-subworkflow-manager.ts L277-287)：
```typescript
private handleSubgraphCompleted(threadId: string, result: ExecutedSubgraphResult): void {
  // ...
  
  const subgraphEntity = this.threadRegistry.get(threadId);  // ⚠️ 可能返回 null
  if (subgraphEntity) {
    // 但仍有其他地方持有该 entity 的引用
    this.unregisterParentChildRelationship(subgraphEntity);
  }
  
  // 清理 TaskRegistry
  const taskInfo = this.taskRegistry.getAll().find(
    t => t.threadEntity.getThreadId() === threadId
  );  // ⚠️ 这里仍然可以访问 threadEntity，即使 ThreadRegistry 已删除
  
  if (taskInfo) {
    this.taskRegistry.delete(taskInfo.id);
  }
}
```

**风险**：
- 使用后删除（Use-After-Free）：某服务删除了引用，其他服务仍在使用
- 数据不一致：多个服务对同一实体的引用状态不同步
- 隐藏的内存泄漏：某些引用可能永久保留导致 GC 无法清理

### 问题2：Graph 的共享引用问题

**当前设计**：
```
PreprocessedGraph (从 GraphRegistry 获取)
    ↓ 由以下持有引用：
    - Thread.graph (直接字段)
    - ThreadEntity 的所有方法 (通过 thread.graph)
    - GraphNavigator (执行时创建)
    
↓ 生命周期冲突：

GraphRegistry.delete(workflowId) 
    ↓
PreprocessedGraph 无法被 GC 清理（仍有线程引用）
或
PreprocessedGraph 被修改时
    ↓
所有引用该图的线程都会看到修改（无隔离）
```

**问题代码** (thread-builder.ts L112)：
```typescript
const thread: Thread = {
  id: threadId,
  workflowId: preprocessedGraph.workflowId,
  workflowVersion: preprocessedGraph.workflowVersion,
  status: 'CREATED' as ThreadStatus,
  currentNodeId: startNode.id,
  graph: threadGraphData,  // ⚠️ 直接引用，非深拷贝
  // ...
};
```

**风险**：
- 如果 PreprocessedGraph 被修改，所有引用它的线程都会受影响
- 不支持工作流的动态更新而不影响运行中的线程
- 线程之间缺乏隔离性

### 问题3：清理顺序不确定

**当前清理流程** (triggered-subworkflow-manager.ts L272-296)：
```
handleSubgraphCompleted()
  ├─ emitCompletedEvent()
  ├─ threadRegistry.get(threadId)         [L277]
  ├─ unregisterParentChildRelationship()
  ├─ taskRegistry.getAll()                [L283]
  ├─ taskRegistry.delete()                [L287]
  ├─ activeTasks.delete()                 [L291]
  └─ callbackManager.triggerCallback()    [L295]
```

**问题**：
- 没有明确的清理顺序
- 多个地方都在清理同一实体的状态
- 没有一个统一的清理入口点

### 问题4：职责边界模糊

| 服务 | 所有权类型 | 清理职责 | 冲突点 |
|------|----------|--------|--------|
| ThreadBuilder | 创建 ThreadEntity | 不清理 | ThreadEntity 的所有者是谁？ |
| ThreadRegistry | 注册引用 | 提供 delete() | 是否应该级联清理相关数据？ |
| TaskRegistry | 注册引用 | 提供 delete() | 何时删除 ThreadEntity 引用？ |
| TriggeredSubworkflowManager | 获取引用 | 参与清理 | 是否有权删除 ThreadEntity？ |
| GraphRegistry | 强所有权 | 拥有清理权 | 何时安全删除 PreprocessedGraph？ |

---

## 第三部分：推荐的改进方案

### 方案1：引用计数所有权模型（推荐）

**核心思想**：
采用引用计数（Reference Counting）模式，明确管理实体的所有权和生命周期。

```typescript
// 伪代码示意

interface OwnedEntity<T> {
  value: T;
  refCount: number;
  owners: Set<string>;  // 追踪所有者
  
  addRef(owner: string): void;
  removeRef(owner: string): void;
  canDelete(): boolean;
}

// ThreadEntity 包装器
class ManagedThreadEntity extends OwnedEntity<ThreadEntity> {
  addRef(owner: string) {
    this.owners.add(owner);
    this.refCount++;
  }
  
  removeRef(owner: string) {
    this.owners.delete(owner);
    this.refCount--;
  }
  
  canDelete(): boolean {
    return this.refCount === 0;
  }
}
```

**所有权关系**：
```
ThreadEntity 的所有者（按优先级）：
1. ThreadRegistry（主所有者）- 拥有删除权
2. TaskRegistry - 引用所有者，删除前必须通知 ThreadRegistry
3. TriggeredSubworkflowManager - 引用所有者，删除前必须通知
4. ExecutionContext - 执行期间的引用所有者

删除流程：
ThreadRegistry.delete(threadId) {
  if (threadEntity.refCount > 0) {
    // 拒绝删除，返回错误
    return false;
  }
  
  // 删除
  this.threadEntities.delete(threadId);
}
```

**优点**：
- ✅ 清晰的所有权关系
- ✅ 防止在引用仍活跃时删除
- ✅ 自动垃圾回收
- ✅ 可追踪引用持有者

**缺点**：
- ⚠️ 增加内存开销
- ⚠️ 需要修改所有持有引用的地方
- ⚠️ 可能出现引用计数泄漏

### 方案2：所有权树结构（推荐用于 Graph）

**核心思想**：
采用树形所有权结构，明确的所有者和被所有者关系。

```
GraphRegistry (强所有权)
    └── PreprocessedGraph
        └── [所有权由 GraphRegistry 持有]

ThreadBuilder (创建权)
    └── ThreadEntity
        └── [所有权由 ThreadRegistry 持有]

ThreadRegistry (强所有权)
    └── ThreadEntity
        └── [关联的弱引用]
            ├── TaskRegistry (弱引用)
            ├── TriggeredSubworkflowManager (弱引用)
            └── ExecutionContext (弱引用)
```

**实现**：
```typescript
// 强所有权模式
class ThreadRegistry {
  private threadEntities: Map<string, ThreadEntity> = new Map();
  private owners: Map<string, Set<string>> = new Map();  // threadId -> ownerIds
  
  register(threadEntity: ThreadEntity, owner: string): void {
    const threadId = threadEntity.getThreadId();
    this.threadEntities.set(threadId, threadEntity);
    
    if (!this.owners.has(threadId)) {
      this.owners.set(threadId, new Set());
    }
    this.owners.get(threadId)!.add(owner);
  }
  
  delete(threadId: string, owner: string): boolean {
    const owners = this.owners.get(threadId);
    
    // 检查是否有其他所有者
    if (owners && owners.size > 1) {
      owners.delete(owner);
      return false;  // 还有其他所有者，不删除
    }
    
    // 所有所有者都已删除
    this.threadEntities.delete(threadId);
    this.owners.delete(threadId);
    return true;
  }
}
```

**优点**：
- ✅ 明确的所有权关系
- ✅ 支持多个弱引用所有者
- ✅ 清晰的删除权责
- ✅ 易于理解和维护

**缺点**：
- ⚠️ 需要跟踪所有者信息
- ⚠️ 弱引用需要额外处理

### 方案3：显式生命周期协调（当前推荐）

**核心思想**：
定义明确的生命周期事件和协调机制，各服务通过事件通信而非直接引用。

```typescript
// 生命周期事件定义
interface ThreadLifecycleEvent {
  type: 'CREATED' | 'EXECUTING' | 'COMPLETED' | 'FAILED' | 'DELETED';
  threadId: string;
  timestamp: number;
  data?: any;
}

// 生命周期协调器
class ThreadLifecycleCoordinator {
  private emitter: EventEmitter<ThreadLifecycleEvent>;
  
  // 负责 ThreadEntity 的完整生命周期
  async createThread(workflowId: string, options?: ThreadOptions): Promise<ThreadEntity> {
    const threadEntity = await this.threadBuilder.build(workflowId, options);
    
    // 注册到各服务
    this.threadRegistry.register(threadEntity);
    
    // 触发创建事件
    this.emitter.emit({
      type: 'CREATED',
      threadId: threadEntity.getThreadId(),
      timestamp: now()
    });
    
    return threadEntity;
  }
  
  async deleteThread(threadId: string, reason: string): Promise<void> {
    const threadEntity = this.threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new Error(`Thread ${threadId} not found`);
    }
    
    // 触发删除前事件
    this.emitter.emit({
      type: 'DELETED',
      threadId,
      timestamp: now(),
      data: { reason }
    });
    
    // 等待所有订阅者清理完毕
    await this.waitForCleanup(threadId);
    
    // 删除
    this.threadRegistry.delete(threadId);
  }
}
```

**所有权关系**：
```
ThreadLifecycleCoordinator (单一所有者和协调者)
  ├── 拥有 ThreadEntity 的完整生命周期
  ├── 通过事件通知其他服务
  └── 等待所有服务确认清理后才最终删除

订阅者：
  ├── ThreadRegistry - 注册/删除线程
  ├── TaskRegistry - 创建/清理任务记录
  ├── TriggeredSubworkflowManager - 处理子工作流
  └── ExecutionContext - 管理执行状态
```

**优点**：
- ✅ 解耦各服务的直接依赖
- ✅ 清晰的单一协调者
- ✅ 易于添加新的生命周期观察者
- ✅ 支持异步清理

**缺点**：
- ⚠️ 需要事件系统支持
- ⚠️ 清理流程变得异步化
- ⚠️ 需要超时保护机制

---

## 第四部分：Graph 共享问题的具体改进方案

### 当前问题

```
Thread.graph 直接引用 PreprocessedGraph
  ↓
多个 Thread 可能引用同一个 PreprocessedGraph
  ↓
如果 PreprocessedGraph 被修改或删除：
  1. 线程之间缺乏隔离
  2. GraphRegistry 无法安全管理生命周期
```

### 改进方案：Graph 的浅拷贝策略

**方案A：线程专属 Graph 副本（深隔离）**

```typescript
// thread-builder.ts
async buildFromPreprocessedGraph(preprocessedGraph: PreprocessedGraph, options: ThreadOptions = {}): Promise<ThreadEntity> {
  // ... 验证代码 ...
  
  // 为该线程创建专属的 Graph 副本（仅拷贝引用）
  const threadGraphData: PreprocessedGraph = {
    // 核心图结构保持引用（节点、边）
    nodes: preprocessedGraph.nodes,         // 共享，不拷贝
    edges: preprocessedGraph.edges,         // 共享，不拷贝
    neighbors: preprocessedGraph.neighbors, // 共享，不拷贝
    reverseNeighbors: preprocessedGraph.reverseNeighbors, // 共享
    
    // 线程专属元数据（拷贝）
    idMapping: { ...preprocessedGraph.idMapping },
    nodeConfigs: new Map(preprocessedGraph.nodeConfigs),
    triggerConfigs: new Map(preprocessedGraph.triggerConfigs),
    
    // 其他元数据保持不变
    graphAnalysis: preprocessedGraph.graphAnalysis,
    validationResult: preprocessedGraph.validationResult,
    topologicalOrder: [...preprocessedGraph.topologicalOrder],
    subgraphRelationships: [...preprocessedGraph.subgraphRelationships],
    workflowId: preprocessedGraph.workflowId,
    workflowVersion: preprocessedGraph.workflowVersion
  };
  
  // 创建线程
  const thread: Thread = {
    // ...
    graph: threadGraphData,
    // ...
  };
  
  // ...
}
```

**优点**：
- ✅ 线程之间图数据隔离
- ✅ GraphRegistry 仍可管理原始 PreprocessedGraph
- ✅ 减少内存重复（共享不可变部分）

**缺点**：
- ⚠️ 仍需拷贝元数据（但通常较小）

**方案B：Graph 引用计数管理**

```typescript
class ManagedPreprocessedGraph {
  private graph: PreprocessedGraph;
  private refCount: number = 0;
  private threadIds: Set<string> = new Set();
  
  addRef(threadId: string): PreprocessedGraph {
    this.threadIds.add(threadId);
    this.refCount++;
    return this.graph;
  }
  
  removeRef(threadId: string): boolean {
    this.threadIds.delete(threadId);
    this.refCount--;
    return this.refCount === 0;
  }
  
  canDelete(): boolean {
    return this.refCount === 0;
  }
}

// GraphRegistry
class GraphRegistry {
  private graphs: Map<string, ManagedPreprocessedGraph>;
  
  get(workflowId: string, threadId: string): PreprocessedGraph | null {
    const managed = this.graphs.get(workflowId);
    if (managed) {
      managed.addRef(threadId);  // 增加引用计数
      return managed.graph;
    }
    return null;
  }
  
  releaseRef(workflowId: string, threadId: string): void {
    const managed = this.graphs.get(workflowId);
    if (managed) {
      if (managed.removeRef(threadId)) {
        // 引用计数为 0，可以安全删除
        this.graphs.delete(workflowId);
      }
    }
  }
}
```

**优点**：
- ✅ 自动垃圾回收
- ✅ 线程感知的生命周期
- ✅ GraphRegistry 可安全删除

**缺点**：
- ⚠️ 需要修改 Graph 获取和释放的所有地方
- ⚠️ 增加复杂性

---

## 第五部分：实现路线图

### Phase 1：添加生命周期协调器（优先级：高）

**目标**：引入中央协调者，明确管理 ThreadEntity 生命周期

**步骤**：
1. 创建 `ThreadLifecycleCoordinator` 类
2. 实现线程创建、执行、清理的统一入口
3. 逐步将 ThreadRegistry、TaskRegistry、TriggeredSubworkflowManager 的清理逻辑迁移到协调器
4. 添加事件通知机制（CREATED、EXECUTING、COMPLETED、DELETED）
5. 添加测试覆盖生命周期的各个阶段

**预期代码位置**：
```
sdk/core/execution/coordinators/thread-lifecycle-coordinator.ts
```

### Phase 2：添加所有权追踪（优先级：中）

**目标**：明确各服务的所有权关系

**步骤**：
1. 在 ThreadRegistry 中添加所有权追踪
2. 修改 TaskRegistry.register() 以通知 ThreadRegistry
3. 修改 TriggeredSubworkflowManager 以通知 ThreadRegistry
4. 实现拒绝删除的保护机制

**预期代码位置**：
```
sdk/core/services/thread-registry.ts (修改)
sdk/core/services/task-registry.ts (修改)
sdk/core/services/triggered-subworkflow-manager.ts (修改)
```

### Phase 3：实现 Graph 浅拷贝策略（优先级：中）

**目标**：为线程提供隔离的 Graph 副本

**步骤**：
1. 修改 ThreadBuilder.buildFromPreprocessedGraph()
2. 为每个线程创建 PreprocessedGraph 的浅拷贝
3. 共享不可变的节点/边数据
4. 拷贝线程专属的元数据

**预期代码位置**：
```
sdk/core/execution/thread-builder.ts (修改)
```

### Phase 4：优化 GraphRegistry 清理（优先级：低）

**目标**：实现 Graph 的引用计数管理

**步骤**：
1. 创建 ManagedPreprocessedGraph 包装器
2. 修改 GraphRegistry 为强引用计数模式
3. 所有 Graph 获取操作必须调用释放
4. 添加清理机制检测泄漏的引用

**预期代码位置**：
```
sdk/core/services/graph-registry.ts (修改)
```

---

## 第六部分：最佳实践建议

### 1. 所有权原则

```typescript
// ✅ 推荐：明确的所有权

class Service {
  // 强所有权 - 负责创建和销毁
  private ownedEntities: Map<string, Entity> = new Map();
  
  create(): Entity {
    const entity = new Entity();
    this.ownedEntities.set(entity.id, entity);
    return entity;
  }
  
  delete(id: string): void {
    this.ownedEntities.delete(id);
  }
  
  // 弱引用 - 不负责生命周期管理
  private weakReferences: Map<string, Entity> = new Map();
  
  registerWeak(entity: Entity): void {
    this.weakReferences.set(entity.id, entity);
  }
  
  getWeak(id: string): Entity | null {
    return this.weakReferences.get(id) || null;
  }
}
```

### 2. 清理顺序规则

```typescript
// 清理顺序（从内向外）：
// 1. ExecutionContext 清理（执行上下文）
// 2. TriggeredSubworkflowManager 清理（子工作流清理）
// 3. TaskRegistry 清理（任务记录清理）
// 4. ThreadRegistry 清理（线程注册清理）
// 5. GraphRegistry 清理（图注册清理，最后）

async cleanup(threadId: string) {
  // 步骤1
  await executionContextManager.cleanup(threadId);
  
  // 步骤2
  await triggeredSubworkflowManager.cleanup(threadId);
  
  // 步骤3
  taskRegistry.cleanup(threadId);
  
  // 步骤4
  threadRegistry.delete(threadId);
  
  // 步骤5
  const thread = this.lastKnownThread.get(threadId);
  if (thread) {
    graphRegistry.releaseRef(thread.workflowId, threadId);
  }
}
```

### 3. 代码审查检查表

- [ ] 是否明确标注了所有权类型（强/弱）？
- [ ] 是否有多个服务尝试删除同一对象？
- [ ] 是否在删除前检查了所有引用？
- [ ] 是否有明确的清理顺序文档？
- [ ] 是否支持可观测性（日志、追踪）？
- [ ] 是否有内存泄漏测试？

---

## 总结与建议

### 关键发现

1. **当前设计缺陷**：多个服务持有相同对象的引用，没有明确的所有权和生命周期管理
2. **主要风险**：Use-After-Free、数据不一致、内存泄漏
3. **Graph 问题**：Thread 直接引用共享的 PreprocessedGraph，缺乏隔离

### 推荐实施方案

**短期（1-2周）：** 实现 ThreadLifecycleCoordinator，集中管理线程生命周期  
**中期（2-4周）：** 添加所有权追踪和 Graph 浅拷贝  
**长期（后续）：** 考虑完全的引用计数系统

### 预期收益

- ✅ 消除 Use-After-Free 风险
- ✅ 改善代码可维护性
- ✅ 提高系统的可观测性
- ✅ 为未来的功能扩展奠定基础

