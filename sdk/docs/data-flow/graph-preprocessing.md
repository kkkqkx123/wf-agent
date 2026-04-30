# Graph 预处理

本文档详细描述从 WorkflowTemplate 到 WorkflowGraph 的预处理流程。

## 概述

预处理是将 WorkflowTemplate 转换为可执行的 WorkflowGraph 的过程，包括验证、构建、分析和优化。

## 核心类型

### WorkflowGraph

**位置**：`sdk/workflow/entities/workflow-graph.ts`

```typescript
class WorkflowGraphData extends GraphData implements WorkflowGraph {
  // 继承自 GraphData 的图结构
  nodes: Map<ID, GraphNode>;                    // 节点映射
  edges: Map<ID, GraphEdge>;                    // 边映射
  adjacencyList: AdjacencyList;                 // 前向邻接表
  reverseAdjacencyList: ReverseAdjacencyList;   // 反向邻接表
  startNodeId?: ID;                             // 起始节点 ID
  endNodeIds: Set<ID>;                          // 终止节点 ID 集合
  
  // ID 映射相关
  idMapping: IdMapping;                         // ID 映射表
  nodeConfigs: Map<ID, unknown>;                // 预处理后的节点配置
  triggerConfigs: Map<ID, unknown>;             // 预处理后的触发器配置
  subgraphRelationships: SubgraphRelationship[]; // 子图关系
  
  // 图分析结果
  graphAnalysis: GraphAnalysisResult;           // 图分析结果
  validationResult: PreprocessValidationResult; // 验证结果
  topologicalOrder: ID[];                       // 拓扑排序结果
  subgraphMergeLogs: SubgraphMergeLog[];        // 子图合并日志
  processedAt: Timestamp;                       // 预处理时间戳
  
  // Workflow 元数据
  workflowId: ID;                               // Workflow ID
  workflowVersion: Version;                     // Workflow 版本
  triggers?: WorkflowTrigger[];                 // 触发器（已展开）
  variables?: WorkflowVariable[];               // 变量定义
  hasSubgraphs: boolean;                        // 是否包含子图
  subworkflowIds: Set<ID>;                      // 子 Workflow ID 集合
}
```

### GraphData

**位置**：`sdk/graph/entities/graph-data.ts`

```typescript
class GraphData implements Graph {
  nodes: NodeMap;                               // 节点集合
  edges: EdgeMap;                               // 边集合
  adjacencyList: AdjacencyList;                 // 前向邻接表
  reverseAdjacencyList: ReverseAdjacencyList;   // 反向邻接表
  startNodeId?: ID;                             // 起始节点
  endNodeIds: Set<ID>;                          // 终止节点集合
}
```

### IdMapping

```typescript
interface IdMapping {
  nodeIds: Map<string, string>;                 // 原始节点 ID → 预处理后节点 ID
  edgeIds: Map<string, string>;                 // 原始边 ID → 预处理后边 ID
  reverseNodeIds: Map<string, string>;          // 反向映射
  reverseEdgeIds: Map<string, string>;          // 反向映射
  subgraphNamespaces: Map<string, string>;      // 子图命名空间
}
```

### GraphAnalysisResult

```typescript
interface GraphAnalysisResult {
  cycleDetection: {                             // 环检测
    hasCycle: boolean;
    cycleNodes: ID[];
    cycleEdges: ID[];
  };
  reachability: {                               // 可达性分析
    reachableFromStart: Set<ID>;
    reachableToEnd: Set<ID>;
    unreachableNodes: Set<ID>;
    deadEndNodes: Set<ID>;
  };
  topologicalSort: {                            // 拓扑排序
    success: boolean;
    sortedNodes: ID[];
    cycleNodes: ID[];
  };
  forkJoinValidation: {                         // Fork/Join 验证
    isValid: boolean;
    unpairedForks: ID[];
    unpairedJoins: ID[];
    pairs: Map<ID, ID>;
  };
  nodeStats: {                                  // 节点统计
    total: number;
    byType: Map<NodeType, number>;
  };
  edgeStats: {                                  // 边统计
    total: number;
    byType: Map<EdgeType, number>;
  };
}
```

---

## 预处理流程

### 整体流程图

```
WorkflowDefinition
       ↓
┌─────────────────────────────────────┐
│     processWorkflow()               │
└─────────────────────────────────────┘
       ↓
┌─────────────────────────────────────┐
│ 1. WorkflowValidator.validate()     │  验证阶段
└─────────────────────────────────────┘
       ↓
┌─────────────────────────────────────┐
│ 2. expandNodeReferences()           │  展开节点引用
│ 3. expandTriggerReferences()        │  展开触发器引用
└─────────────────────────────────────┘
       ↓
┌─────────────────────────────────────┐
│ 4. GraphBuilder.buildAndValidate()  │  构建图
│ 5. processSubgraphs()               │  处理子图
└─────────────────────────────────────┘
       ↓
┌─────────────────────────────────────┐
│ 6. GraphValidator.validate()        │  验证图
│ 7. GraphValidator.analyze()         │  分析图
└─────────────────────────────────────┘
       ↓
┌─────────────────────────────────────┐
│ 8. IdMappingBuilder.build()         │  生成 ID 映射
└─────────────────────────────────────┘
       ↓
┌─────────────────────────────────────┐
│ 9. 创建 WorkflowGraphData             │  组装结果
└─────────────────────────────────────┘
       ↓
WorkflowGraph
```

### 详细步骤

#### 1. 验证 WorkflowDefinition

**位置**：`sdk/workflow/validation/workflow-validator.ts`

```typescript
const validationResult = validator.validate(workflow);
if (validationResult.isErr()) {
  throw new ConfigurationValidationError(...);
}
```

验证内容：
- 基本字段完整性
- 节点类型有效性
- 边连接有效性
- 配置有效性

#### 2. 展开节点引用

**位置**：`sdk/graph/graph-builder/workflow-processor.ts`

```typescript
function expandNodeReferences(nodes: Node[]): Node[] {
  const expandedNodes: Node[] = [];
  
  for (const node of nodes) {
    if (isNodeReference(node)) {
      // 获取节点模板
      const template = nodeTemplateRegistry.get(templateName);
      
      // 合并配置覆盖
      const mergedConfig = { ...template.config, ...configOverride };
      
      // 创建展开后的节点
      const expandedNode = {
        id: nodeId,
        type: template.type,
        name: nodeName || template.name,
        config: mergedConfig,
        ...
      };
      
      expandedNodes.push(expandedNode);
    } else {
      expandedNodes.push(node);
    }
  }
  
  return expandedNodes;
}
```

#### 3. 展开触发器引用

```typescript
function expandTriggerReferences(
  triggers: (WorkflowTrigger | TriggerReference)[]
): WorkflowTrigger[] {
  const expandedTriggers: WorkflowTrigger[] = [];
  
  for (const trigger of triggers) {
    if (isTriggerReference(trigger)) {
      // 使用 TriggerTemplateRegistry 转换
      const workflowTrigger = triggerTemplateRegistry.convertToWorkflowTrigger(
        reference.templateName,
        reference.triggerId,
        reference.triggerName,
        reference.configOverride
      );
      expandedTriggers.push(workflowTrigger);
    } else {
      expandedTriggers.push(trigger);
    }
  }
  
  return expandedTriggers;
}
```

#### 4. 构建图

**位置**：`sdk/graph/graph-builder/graph-builder.ts`

```typescript
const buildResult = GraphBuilder.buildAndValidate(expandedWorkflow, buildOptions);
if (!buildResult.isValid) {
  throw new ConfigurationValidationError(...);
}
```

构建内容：
- 创建 GraphData 实例
- 添加所有节点
- 添加所有边
- 构建邻接表
- 识别起始和终止节点

#### 5. 处理子图

```typescript
const subgraphResult = await GraphBuilder.processSubgraphs(
  buildResult.graph,
  options.workflowRegistry,
  options.maxRecursionDepth ?? 10
);

if (!subgraphResult.success) {
  throw new ConfigurationValidationError(...);
}
```

处理内容：
- 识别 SUBGRAPH 类型节点
- 递归预处理子 Workflow
- 合并子图到主图
- 记录子图合并日志

#### 6. 验证图

**位置**：`sdk/graph/validation/graph-validator.ts`

```typescript
const graphValidationResult = GraphValidator.validate(buildResult.graph);
if (graphValidationResult.isErr()) {
  throw new ConfigurationValidationError(...);
}
```

验证内容：
- 节点连接完整性
- 起始节点唯一性
- 终止节点存在性
- Fork/Join 配对

#### 7. 分析图

```typescript
const graphAnalysis = GraphValidator.analyze(buildResult.graph);
```

分析内容：
- 环检测
- 可达性分析
- 拓扑排序
- Fork/Join 验证
- 节点和边统计

#### 8. 生成 ID 映射

**位置**：`sdk/graph/graph-builder/id-mapping-builder.ts`

```typescript
const idMappingResult = await idMappingBuilder.build(buildResult.graph, expandedWorkflow);
```

生成内容：
- 原始 ID 到预处理后 ID 的映射
- 反向映射
- 子图命名空间
- 更新节点配置中的 ID 引用

#### 9. 组装结果

```typescript
const workflowGraph = new WorkflowGraphData();

// 复制图结构
workflowGraph.nodes = buildResult.graph.nodes;
workflowGraph.edges = buildResult.graph.edges;
workflowGraph.adjacencyList = buildResult.graph.adjacencyList;
workflowGraph.reverseAdjacencyList = buildResult.graph.reverseAdjacencyList;
workflowGraph.startNodeId = buildResult.graph.startNodeId;
workflowGraph.endNodeIds = buildResult.graph.endNodeIds;

// 设置 ID 映射
workflowGraph.idMapping = idMappingResult.idMapping;
workflowGraph.nodeConfigs = idMappingResult.nodeConfigs;
workflowGraph.triggerConfigs = idMappingResult.triggerConfigs;
workflowGraph.subgraphRelationships = idMappingResult.subgraphRelationships;

// 设置分析结果
workflowGraph.graphAnalysis = graphAnalysis;
workflowGraph.validationResult = preprocessValidation;
workflowGraph.topologicalOrder = graphAnalysis.topologicalSort.sortedNodes;

// 设置 Workflow 元数据
workflowGraph.workflowId = expandedWorkflow.id;
workflowGraph.workflowVersion = expandedWorkflow.version;
workflowGraph.triggers = expandedTriggers;
workflowGraph.variables = expandedWorkflow.variables;
workflowGraph.hasSubgraphs = hasSubgraphs;
workflowGraph.subworkflowIds = subworkflowIds;

return workflowGraph;
```

---

## GraphRegistry

### 职责

WorkflowGraphRegistry 负责 WorkflowGraph 的存储和查询。

**位置**：`sdk/workflow/stores/workflow-graph-registry.ts`

### 主要方法

```typescript
class WorkflowGraphRegistry {
  private graphs: Map<string, WorkflowGraph> = new Map();
  
  // 注册预处理图
  register(graph: WorkflowGraph): void {
    this.graphs.set(graph.workflowId, graph);
  }
  
  // 获取预处理图
  get(workflowId: string): WorkflowGraph | undefined {
    return this.graphs.get(workflowId);
  }
  
  // 检查是否存在
  has(workflowId: string): boolean {
    return this.graphs.has(workflowId);
  }
  
  // 删除预处理图
  delete(workflowId: string): void {
    this.graphs.delete(workflowId);
  }
  
  // 清空所有
  clear(): void {
    this.graphs.clear();
  }
}
```

---

## 图导航

### WorkflowNavigator

**位置**：`sdk/workflow/builder/workflow-navigator.ts`

提供图的导航功能：

```typescript
class WorkflowNavigator {
  private graph: WorkflowGraph;
  
  // 获取下一个节点
  getNextNode(currentNodeId: string): { nextNodeId: string; edge: WorkflowEdge } | null {
    const outgoingEdges = this.graph.getOutgoingEdges(currentNodeId);
    // 根据边条件选择下一个节点
    // 返回下一个节点 ID 和边信息
  }
  
  // 获取图
  getGraph(): WorkflowGraph {
    return this.graph;
  }
}
```

---

## 预处理优化

### 1. 拓扑排序

用于确定节点的执行顺序：

```typescript
topologicalSort: {
  success: boolean;
  sortedNodes: ID[];      // 拓扑排序结果
  cycleNodes: ID[];       // 环中的节点
}
```

### 2. 可达性分析

识别不可达节点和死端节点：

```typescript
reachability: {
  reachableFromStart: Set<ID>;    // 从起始节点可达的节点
  reachableToEnd: Set<ID>;        // 可达终止节点的节点
  unreachableNodes: Set<ID>;      // 不可达节点
  deadEndNodes: Set<ID>;          // 死端节点
}
```

### 3. 环检测

检测图中的环：

```typescript
cycleDetection: {
  hasCycle: boolean;
  cycleNodes: ID[];
  cycleEdges: ID[];
}
```

---

## 设计原则

### 1. 不可变性

- WorkflowGraph 创建后不可修改
- 确保执行期间图结构稳定
- 支持多 WorkflowExecution 共享

### 2. 完整性

- 预处理阶段完成所有验证
- 运行时不需要重复验证
- 失败时提供详细错误信息

### 3. 优化

- 预计算邻接表
- 预计算拓扑排序
- 预分析可达性

### 4. 可追溯性

- ID 映射保留原始 ID
- 子图合并日志记录合并过程
- 验证结果记录验证时间

---

## 相关文档

- [整体数据流](./README.md)
- [Workflow 定义与管理](./workflow-definition.md)
- [Workflow 执行实例](./workflow-execution.md)
