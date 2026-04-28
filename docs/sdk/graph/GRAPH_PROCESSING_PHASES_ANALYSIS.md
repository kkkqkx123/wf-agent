# 图处理阶段分析

## 核心发现

**图处理是多阶段的，并不仅限于预处理阶段**。GraphData 的生命周期贯穿工作流的整个执行周期。

## 图处理的完整生命周期

```
┌─────────────────────────────────────────────────────────────────┐
│                   工作流生命周期与图处理阶段                        │
└─────────────────────────────────────────────────────────────────┘

1. 工作流定义 (WorkflowDefinition)
   ↓
2. 【预处理阶段】图创建与扩展
   ├─ GraphBuilder.build()           → 基础图结构创建
   ├─ GraphBuilder.processSubgraphs() → 子工作流递归合并
   ├─ GraphValidator.validate()       → 拓扑验证
   └─ GraphValidator.analyze()        → 拓扑分析
   ↓
3. 【执行准备阶段】图初始化
   ├─ ThreadBuilder.build()                  → 获取预处理工作流
   ├─ ThreadContext.buildFromProcessedDefinition() → 图附加到线程
   └─ GraphNavigator 初始化                 → 基于图构建导航器
   ↓
4. 【执行阶段】图导航与查询
   ├─ GraphNavigator.getNextNode()        → 确定下一节点
   ├─ GraphNavigator.getRoutingDecision() → 路由决策
   ├─ GraphNavigator.getNodeInfo()        → 节点信息查询
   └─ graph.getNode/getEdge()             → 节点边查询
   ↓
5. 【检查点/恢复阶段】图持久化与重建
   ├─ CheckpointStateManager.serialize()   → 图序列化
   ├─ CheckpointStateManager.deserialize() → 图反序列化
   └─ CheckpointCoordinator.inferForkJoinState() → 使用图推断状态
```

## 详细的阶段分析

### 阶段 1：预处理阶段（主要图处理）

#### 1.1 图创建 - GraphBuilder.build()

**位置**: `sdk/core/graph/graph-builder.ts` (L30-72)

**职责**: 从 WorkflowDefinition 创建初始的 GraphData

```typescript
// 伪代码流程
static build(workflow: WorkflowDefinition): GraphData {
  const graph = new GraphData();  // 创建空图
  
  // 添加节点
  for (const node of workflow.nodes) {
    graph.addNode({
      id: node.id,
      type: node.type,
      originalNode: node,
      ...
    });
    // 记录 START/END 节点特殊标记
    if (node.type === 'START') {
      graph.startNodeId = node.id;
    } else if (node.type === 'END') {
      graph.endNodeIds.add(node.id);
    }
  }
  
  // 添加边
  for (const edge of workflow.edges) {
    graph.addEdge({
      sourceNodeId: edge.sourceNodeId,
      targetNodeId: edge.targetNodeId,
      ...
    });
  }
  
  return graph;
}
```

**关键点**:
- 创建基础的单工作流图结构
- 维护节点的拓扑顺序列表（adjancencyList）
- 建立双向边引用关系

#### 1.2 子工作流合并 - GraphBuilder.processSubgraphs()

**位置**: `sdk/core/graph/graph-builder.ts` (L171-299, L309-448)

**职责**: 递归处理并合并子工作流到主图

```typescript
// 关键处理步骤
processSubgraphs(mainGraph: GraphData, subgraphNode: Node) {
  1. 获取子工作流定义
  2. 递归构建子图 (build())
  3. 递归处理子图的子工作流
  4. 合并子图节点和边到主图
     ├─ 重命名节点 ID（避免冲突）
     ├─ 重命名边 ID
     ├─ 保留源/目标节点关系
     └─ 更新 SUBGRAPH 节点的入出边引用
  5. 处理边界节点
     ├─ 连接主图中 SUBGRAPH 节点的入边到子图入口
     └─ 连接子图出口到 SUBGRAPH 节点的出边
}
```

**处理流程图**:
```
主工作流中的 SUBGRAPH 节点
  ↓
获取子工作流定义
  ↓
递归构建子图结构
  ↓
合并节点和边（ID 重命名）
  ↓
处理边界连接
  ├─ 入边：主图 → 子图入口节点
  └─ 出边：子图出口节点 → 主图
  ↓
返回合并后的主图
```

**并发特性**: 支持多个子工作流的并行合并（使用 Promise.all）

#### 1.3 图验证 - GraphValidator.validate()

**位置**: `sdk/core/validation/graph-validator.ts` (L57-163)

**验证项和执行时机**:

| 验证项 | 执行点 | 何时触发 |
|--------|--------|--------|
| START/END 存在性 | L78-83 | 每次验证 |
| 孤立节点 | L86-89 | 每次验证 |
| 循环依赖 | L92-103 | 需要时（默认启用） |
| 可达性 | L106-135 | 需要时（默认启用） |
| FORK/JOIN 配对 | L138-141 | 需要时（默认启用） |
| 节点边一致性 | L155-157 | 总是执行（强制） |

**在 GraphBuilder 中的使用** (L92-98):
```typescript
const validationResult = GraphValidator.validate(graph, {
  checkCycles: options.detectCycles,
  checkReachability: options.analyzeReachability,
  checkForkJoin: true,
  checkStartEnd: true,
  checkIsolatedNodes: true,
});
```

#### 1.4 图分析 - GraphValidator.analyze()

**位置**: `sdk/core/validation/graph-validator.ts` (定义处理分析方法)

**分析内容**:
- 拓扑排序（`GraphTopologicalSorter.sort()`）
- 图统计信息（`analyzeGraph()`）
- 可达性分析（`analyzeReachability()`）

**在 WorkflowProcessor 中的使用** (L162):
```typescript
const graphAnalysis = GraphValidator.analyze(buildResult.graph);
```

### 阶段 2：执行准备阶段

#### 2.1 ThreadBuilder 中的图初始化

**位置**: `sdk/core/execution/thread-builder.ts` (L55-73)

**流程**:
```typescript
async build(workflowId: string, options: ThreadOptions) {
  // 获取预处理后的工作流定义（包含 graph）
  const processedWorkflow = await this.workflowRegistry.ensureProcessed(workflowId);
  
  // 从预处理定义构建线程
  return this.buildFromProcessedDefinition(processedWorkflow, options);
}

private buildFromProcessedDefinition(processedWorkflow: ProcessedWorkflowDefinition) {
  // 从 processedWorkflow.graph 获取图数据
  const threadGraphData = processedWorkflow.graph;
  
  // 创建 ThreadContext，并将图附加到 Thread 对象
  const thread = new Thread({
    ...
    graph: threadGraphData,  // 图成为 Thread 的成员
    ...
  });
  
  return threadContext;
}
```

**关键点**:
- 图是从缓存的预处理工作流中获取的（避免重复处理）
- 图作为不可变的定义数据附加到 Thread 对象
- 所有线程共享同一个图定义（节省内存）

#### 2.2 GraphNavigator 初始化

**位置**: `sdk/core/execution/context/thread-context.ts`

**初始化**:
```typescript
this.navigator = new GraphNavigator(this.thread.graph);
```

**GraphNavigator 设计原则**:
- **无状态执行**: 不缓存执行流程状态（如 currentNodeId）
- **有状态定义**: 持有不变的 GraphData
- **纯函数**: 所有方法都是纯函数，不产生副作用

### 阶段 3：执行阶段（连续使用图）

#### 3.1 图导航 - GraphNavigator

**位置**: `sdk/core/graph/graph-navigator.ts`

**核心导航方法**:

**3.1.1 getNextNode() - 确定下一个节点**
```typescript
// 用于标准顺序执行
getNextNode(currentNodeId?: ID): NavigationResult {
  // 获取出边
  const outgoingEdges = this.graph.getOutgoingEdges(currentNodeId);
  
  // 返回导航结果（包含是否到达END、多路径信息等）
  if (outgoingEdges.length === 0) {
    return { isEnd: true, ... };
  } else if (outgoingEdges.length === 1) {
    return { nextNodeId: targetNode, ... };
  } else {
    return { hasMultiplePaths: true, possibleNextNodeIds: [...], ... };
  }
}
```

**3.1.2 getRoutingDecision() - 路由决策**
```typescript
// 用于多路径场景（条件分支、FORK 等）
getRoutingDecision(currentNodeId: ID, context: any): RoutingDecision {
  const outgoingEdges = this.graph.getOutgoingEdges(currentNodeId);
  
  // 根据边的条件和上下文选择路径
  for (const edge of outgoingEdges) {
    if (evaluateCondition(edge.condition, context)) {
      return {
        selectedNodeId: edge.targetNodeId,
        edgeId: edge.id,
        reason: 'condition matched'
      };
    }
  }
}
```

**3.1.3 getNodeInfo() - 节点信息查询**
```typescript
// 执行时获取节点信息
getNodeInfo(nodeId: ID) {
  const node = this.graph.getNode(nodeId);
  return {
    id: node.id,
    type: node.type,
    config: node.config,
    ...
  };
}
```

**执行时使用频率**: 每个节点执行都需要 1-3 次图查询

#### 3.2 检查点恢复阶段中的图使用

**位置**: `sdk/core/execution/coordinators/checkpoint-coordinator.ts`

**3.2.1 推断 FORK/JOIN 状态** (L239-245)
```typescript
if (thread.graph) {
  const currentNode = thread.graph.getNode(checkpoint.threadState.currentNodeId);
  if (currentNode && currentNode.type === 'JOIN') {
    const forkJoinState = this.inferForkJoinState(
      checkpoint.threadState.currentNodeId,
      checkpoint.threadState.nodeResults,
      thread.graph
    );
  }
}
```

**3.2.2 推断分支状态** (L323)
```typescript
const forkNode = graph.getNode(forkNodeId);
if (!forkNode || forkNode.type !== 'FORK') {
  return { completedPaths: new Set(), pendingPaths: new Set() };
}

// 从 FORK 节点配置和执行结果推断分支状态
const forkPaths = (forkNode.config as any)?.forkPaths || [];
```

### 阶段 4：持久化阶段

#### 4.1 图的序列化和反序列化

**位置**: `sdk/core/execution/managers/checkpoint-state-manager.ts`

**序列化过程** (L140-173):
```typescript
// GraphData 通过 Thread 对象被序列化
// Thread 包含 graph 字段
const serialized = {
  ...thread,
  graph: thread.graph  // GraphData 直接序列化
};
```

**反序列化过程**:
```typescript
// 恢复时 GraphData 从序列化数据重建
const restored = {
  ...deserializedThread,
  graph: new GraphData() // 或从序列化数据重建
};
```

**关键特性**:
- GraphData 是可序列化的（标准 JSON）
- 图数据在检查点中是完整的（用于恢复后的导航）
- 大图可能增加检查点大小（性能考量）

## 图处理的数据流

```
┌──────────────────────────────────────────────────────────────────┐
│  工作流输入 (WorkflowDefinition + 子工作流定义)                      │
└──────────────────────────────────────────────────────────────────┘
        ↓
┌──────────────────────────────────────────────────────────────────┐
│  【预处理阶段】                                                      │
│  GraphBuilder.build() ──→ 创建初始图结构                           │
│     ↓                                                               │
│  GraphBuilder.processSubgraphs() ──→ 递归合并子工作流              │
│     ↓                                                               │
│  GraphValidator.validate() ──→ 拓扑验证                           │
│     ↓                                                               │
│  GraphValidator.analyze() ──→ 拓扑分析                            │
│     ↓                                                               │
│  ProcessedWorkflowDefinition (包含完整的 GraphData)               │
└──────────────────────────────────────────────────────────────────┘
        ↓
┌──────────────────────────────────────────────────────────────────┐
│  【缓存存储】WorkflowRegistry                                        │
│  （避免重复预处理，所有线程共享同一个图）                            │
└──────────────────────────────────────────────────────────────────┘
        ↓
┌──────────────────────────────────────────────────────────────────┐
│  【执行准备阶段】                                                    │
│  ThreadBuilder.build() ──→ 获取缓存的预处理工作流                 │
│     ↓                                                               │
│  ThreadContext.buildFromProcessedDefinition()                   │
│     ├─ 将图附加到 Thread 对象                                      │
│     └─ 初始化 GraphNavigator                                      │
│     ↓                                                               │
│  ThreadContext (包含 graph、navigator、currentNodeId 等)        │
└──────────────────────────────────────────────────────────────────┘
        ↓
┌──────────────────────────────────────────────────────────────────┐
│  【执行阶段】（循环执行）                                            │
│  1. getCurrentNode() ──→ graph.getNode(currentNodeId)            │
│  2. executeNode() ──→ 执行当前节点                                 │
│  3. getNextNode() ──→ graph.getOutgoingEdges(currentNodeId)      │
│  4. updateCurrentNode() ──→ 更新 currentNodeId                   │
│  5. 返回步骤 1（直到到达 END 节点或出错）                          │
│                                                                    │
│  特殊情况：                                                        │
│  - FORK 节点 ──→ graph.getNode(forkNodeId) → getRoutingDecision │
│  - JOIN 节点 ──→ waitForAllBranches() → inferForkJoinState()    │
│  - 条件分支 ──→ getRoutingDecision() 使用图的边信息             │
└──────────────────────────────────────────────────────────────────┘
        ↓
┌──────────────────────────────────────────────────────────────────┐
│  【检查点/恢复阶段】                                                  │
│  1. saveCheckpoint()                                               │
│     ├─ 序列化 thread（包含 graph）                                 │
│     └─ 存储检查点数据                                              │
│  2. restoreFromCheckpoint()                                        │
│     ├─ 反序列化 thread（恢复 graph）                               │
│     ├─ 重建 GraphNavigator                                         │
│     └─ 推断 FORK/JOIN 状态（使用 graph）                         │
│  3. 恢复执行（返回执行阶段）                                        │
└──────────────────────────────────────────────────────────────────┘
```

## 图处理的性能特征

### 1. 时间复杂度分析

| 操作 | 复杂度 | 何时执行 |
|------|--------|--------|
| 图创建 (build) | O(V + E) | 一次（预处理） |
| 子工作流合并 | O(V + E) × D | 一次，D 是深度 |
| 拓扑验证 | O(V + E) | 一次（预处理） |
| 图导航 (getNextNode) | O(出度) | 每个节点执行 |
| 节点信息查询 | O(1) | 执行时按需 |
| 图序列化 | O(V + E) | 保存检查点时 |

### 2. 空间复杂度分析

| 数据结构 | 大小 | 用途 |
|---------|------|------|
| 节点映射 | O(V) | 快速查询节点 |
| 邻接表 | O(V + E) | 快速查询出边 |
| 反向邻接表 | O(V + E) | 快速查询入边 |
| 可达性缓存 | O(V²) | 可选，用于优化 |

### 3. 缓存策略

```
WorkflowRegistry 中的缓存：
┌─────────────────────────────────────────┐
│ workflowId → ProcessedWorkflowDefinition │
│               ├─ expandedDefinition      │
│               ├─ graph (GraphData)       │
│               ├─ analysis (拓扑信息)    │
│               └─ validation (验证结果)   │
└─────────────────────────────────────────┘

效果：
- 避免重复预处理和验证
- 多个线程共享同一个图定义
- 节约内存和 CPU 时间
```

## 关键设计决策

### 1. 图的不可变性

**设计原则**: GraphData 在创建后不可修改

**原因**:
- 线程之间共享同一个图
- 避免执行时的并发修改问题
- 确保检查点的正确性

**实现**:
- 预处理时完整构建所有信息
- 执行时只读取图的数据
- 执行状态（currentNodeId 等）存储在 ThreadContext，不在 GraphData 中

### 2. 双阶段处理

```
    ┌─────────────────┐
    │  预处理阶段      │
    │ (计算密集)       │
    │ - 构建图结构     │
    │ - 合并子工作流   │
    │ - 验证拓扑       │
    │ - 分析特性       │
    └────────┬────────┘
             ↓ 缓存
    ┌─────────────────┐
    │  执行阶段        │
    │ (I/O 密集)      │
    │ - 执行节点       │
    │ - 导航图结构     │
    │ - 推断状态       │
    └─────────────────┘
```

**优势**:
- 预处理的计算密集操作只做一次
- 执行时的图操作都是 O(1) 或 O(出度) 级别
- 支持批量预处理优化（如离线处理）

### 3. 分离关注点

```
GraphData          → 不可变的定义数据（结构）
ThreadContext      → 可变的执行状态（状态）
GraphNavigator     → 导航逻辑（行为）
```

## 总结：图处理并非仅在预处理阶段

虽然预处理阶段是图处理最密集的地方（构建、合并、验证、分析），但图处理实际上贯穿整个工作流生命周期：

| 阶段 | 图处理内容 | 特点 |
|------|-----------|------|
| **预处理** | 构建、合并、验证、分析 | 计算密集，一次性 |
| **执行准备** | 初始化、缓存获取 | 配置阶段 |
| **执行** | 导航、查询、路由决策 | 频繁但轻量级 |
| **检查点/恢复** | 序列化、推断状态 | 持久化和状态恢复 |

**预处理的优化作用**:
- 将重操作（拓扑验证、分析）推到预处理阶段
- 执行时只做轻量级的查询操作
- 通过缓存避免重复处理

这种设计使得工作流执行既能保证正确性，又能保持高效率。
