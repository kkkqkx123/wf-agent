# 整体数据流与概念边界

本文档描述 Modular Agent Framework 中 workflow 相关的核心概念边界及整体数据流。

## 核心概念边界

### 1. WorkflowDefinition（工作流定义）

**定义**：静态的工作流定义，描述工作流的结构和配置。

**职责**：
- 定义工作流的节点（nodes）和边（edges）
- 定义变量（variables）、触发器（triggers）
- 定义配置选项（config）和元数据（metadata）

**位置**：`packages/types/src/workflow/definition.ts`

**生命周期**：
- 创建 → 注册到 WorkflowRegistry → 预处理 → 转换为 PreprocessedGraph

**关键特征**：
- 纯静态数据，不包含运行时状态
- 可序列化，可持久化
- 可被多个 WorkflowExecution 实例共享

### 2. PreprocessedGraph（预处理图）

**定义**：WorkflowDefinition 经过预处理后生成的可执行图结构。

**职责**：
- 存储图结构（nodes、edges、邻接表）
- 存储 ID 映射（idMapping）
- 存储图分析结果（graphAnalysis）
- 存储拓扑排序结果（topologicalOrder）
- 存储验证结果（validationResult）

**位置**：`sdk/graph/entities/preprocessed-graph-data.ts`

**生命周期**：
- 由 processWorkflow() 从 WorkflowDefinition 生成
- 注册到 GraphRegistry
- 被 WorkflowExecution 引用

**关键特征**：
- 不可变数据结构
- 包含完整的图导航信息
- 可被多个 WorkflowExecution 实例共享

### 3. WorkflowExecution（工作流执行实例）

**定义**：Workflow 的一次执行实例，包含运行时数据。

**职责**：
- 存储当前执行位置（currentNodeId）
- 存储输入输出数据（input、output）
- 存储变量作用域（variableScopes）
- 存储执行历史（nodeResults）
- 存储错误信息（errors）

**位置**：`packages/types/src/workflow/definition.ts`

**生命周期**：
- 由 WorkflowExecutionBuilder 从 WorkflowGraph 创建
- 注册到 WorkflowExecutionRegistry
- 执行 → 完成/失败/取消

**关键特征**：
- 包含运行时状态
- 每次执行创建新实例
- 可序列化（用于检查点）

### 4. WorkflowExecutionEntity（工作流执行实体）

**定义**：WorkflowExecution 的实体封装，提供数据访问接口和状态管理。

**职责**：
- 封装 WorkflowExecution 数据对象
- 持有 WorkflowExecutionState（运行时状态）
- 持有 ExecutionState（子图执行栈）
- 持有 MessageHistory（消息历史）
- 持有 VariableState（变量状态）
- 持有 ConversationSession（对话会话）

**位置**：`sdk/workflow/entities/workflow-execution-entity.ts`

**生命周期**：
- 由 WorkflowExecutionBuilder 创建
- 由 WorkflowLifecycleCoordinator 管理生命周期
- 执行完成后可被清理

**关键特征**：
- 纯数据实体，不包含业务逻辑
- 提供数据访问的 getter/setter
- 持有多个状态管理器

---

## 概念关系图

```
┌─────────────────────────────────────────────────────────────────┐
│                     静态定义层                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  WorkflowDefinition                                             │
│  ├─ id, name, version                                           │
│  ├─ nodes: Node[]                                               │
│  ├─ edges: Edge[]                                               │
│  ├─ variables: WorkflowVariable[]                               │
│  ├─ triggers: WorkflowTrigger[]                                 │
│  └─ config: WorkflowConfig                                      │
│                                                                 │
└───────────────────────────┬─────────────────────────────────────┘
                            │
                            │ processWorkflow()
                            │ (验证、构建、ID映射)
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│                     预处理层                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  PreprocessedGraph (extends Graph)                              │
│  ├─ nodes: Map<ID, GraphNode>                                   │
│  ├─ edges: Map<ID, GraphEdge>                                   │
│  ├─ adjacencyList: Map<ID, Set<ID>>                             │
│  ├─ idMapping: IdMapping                                        │
│  ├─ graphAnalysis: GraphAnalysisResult                          │
│  ├─ topologicalOrder: ID[]                                      │
│  ├─ validationResult: PreprocessValidationResult                │
│  └─ workflowId, workflowVersion                                 │
│                                                                 │
└───────────────────────────┬─────────────────────────────────────┘
                            │
                            │ WorkflowExecutionBuilder.build()
                            │ (创建执行实例)
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│                     运行时层                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  WorkflowExecutionEntity                                        │
│  ├─ workflowExecution: WorkflowExecution                        │
│  │   ├─ id, workflowId, currentNodeId                           │
│  │   ├─ graph: WorkflowGraph (引用)                             │
│  │   ├─ input, output                                           │
│  │   ├─ variableScopes: VariableScopes                          │
│  │   ├─ nodeResults: NodeExecutionResult[]                      │
│  │   └─ errors: unknown[]                                       │
│  ├─ state: WorkflowExecutionState (运行时状态)                   │
│  ├─ executionState: ExecutionState (子图栈)                      │
│  ├─ messageHistoryManager: MessageHistory                       │
│  ├─ variableStateManager: VariableState                         │
│  └─ conversationManager: ConversationSession                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 整体数据流

### 1. 定义阶段

```
用户定义 WorkflowDefinition
         ↓
WorkflowRegistry.register(workflow)
         ↓
WorkflowValidator.validate(workflow)
         ↓
存储到 WorkflowRegistry.workflows
```

### 2. 预处理阶段

```
WorkflowRegistry.registerAsync(workflow)
         ↓
processWorkflow(workflow, options)
         ↓
    [验证]
         ├─ WorkflowValidator.validate()
         ├─ expandNodeReferences()
         └─ expandTriggerReferences()
         ↓
    [构建]
         ├─ GraphBuilder.buildAndValidate()
         └─ processSubgraphs()
         ↓
    [分析]
         ├─ GraphValidator.validate()
         └─ GraphValidator.analyze()
         ↓
    [ID映射]
         └─ IdMappingBuilder.build()
         ↓
PreprocessedGraph
         ↓
GraphRegistry.register(preprocessedGraph)
```

### 3. 实例化阶段

```
WorkflowLifecycleCoordinator.execute(workflowId, options)
         ↓
WorkflowExecutionBuilder.build(workflowId, options)
         ↓
GraphRegistry.get(workflowId) → WorkflowGraph
         ↓
创建 WorkflowExecution 对象
         ├─ 生成 executionId
         ├─ 引用 WorkflowGraph
         ├─ 初始化 variableScopes
         └─ 设置初始 currentNodeId
         ↓
创建 WorkflowExecutionEntity
         ├─ new WorkflowExecutionState()
         ├─ new ExecutionState()
         ├─ new MessageHistory()
         ├─ new VariableState()
         └─ new ConversationSession()
         ↓
WorkflowExecutionRegistry.register(workflowExecutionEntity)
```

### 4. 执行阶段

```
WorkflowExecutor.executeWorkflow(workflowExecutionEntity)
         ↓
WorkflowExecutionCoordinator.execute()
         ↓
    [执行循环]
         ├─ 检查中断状态
         ├─ 获取当前节点
         ├─ NodeExecutionCoordinator.executeNode()
         │       ├─ 触发 NODE_STARTED 事件
         │       ├─ 执行 BEFORE_EXECUTE Hook
         │       ├─ 执行节点逻辑
         │       ├─ 执行 AFTER_EXECUTE Hook
         │       └─ 触发 NODE_COMPLETED 事件
         ├─ 记录执行结果
         └─ 移动到下一节点
         ↓
构建 WorkflowExecutionResult
         ↓
更新 WorkflowExecutionEntity 状态
```

### 5. 生命周期管理

```
WorkflowLifecycleCoordinator
         ├─ execute() → 创建并执行
         ├─ pauseWorkflowExecution() → 暂停
         ├─ resumeWorkflowExecution() → 恢复
         └─ stopWorkflowExecution() → 停止
         ↓
WorkflowStateTransitor
         ├─ startThread()
         ├─ pauseThread()
         ├─ resumeThread()
         ├─ completeThread()
         ├─ failThread()
         └─ cancelThread()
```

---

## 存储层次

```
┌─────────────────────────────────────────────────────────────┐
│  WorkflowRegistry                                            │
│  存储位置: sdk/graph/stores/workflow-registry.ts             │
│  数据: Map<string, WorkflowDefinition>                       │
│  职责: Workflow 定义的注册、查询、管理                         │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  GraphRegistry                                               │
│  存储位置: sdk/graph/stores/graph-registry.ts                │
│  数据: Map<string, PreprocessedGraph>                        │
│  职责: 预处理图的注册、查询、管理                               │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  WorkflowExecutionRegistry                                   │
│  存储位置: sdk/workflow/stores/workflow-execution-registry.ts│
│  数据: Map<string, WorkflowExecutionEntity>                  │
│  职责: WorkflowExecution 实例的注册、查询、管理                │
└─────────────────────────────────────────────────────────────┘
```

---

## 关键设计原则

### 1. 分层分离

- **定义层**：静态的 WorkflowTemplate，可序列化、可共享
- **预处理层**：不可变的 WorkflowGraph，优化后的执行结构
- **运行时层**：WorkflowExecutionEntity，包含执行状态和运行时数据

### 2. 数据不可变性

- WorkflowTemplate：创建后不可修改
- WorkflowGraph：预处理后不可修改
- WorkflowExecution：执行过程中可修改，但结构稳定

### 3. 引用共享

- 多个 WorkflowExecution 可以引用同一个 WorkflowGraph
- WorkflowGraph 引用 WorkflowTemplate 的 ID
- 减少内存占用，提高性能

### 4. 状态封装

- WorkflowExecutionEntity 封装所有运行时状态
- 通过 getter/setter 访问数据
- 状态管理器分离关注点

---

## 文档索引

- [Workflow 定义与管理](./workflow-definition.md)
- [Graph 预处理](./graph-preprocessing.md)
- [Workflow 执行实例](./workflow-execution.md)
