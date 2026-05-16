# SUBGRAPH 与 EmbedGraph 架构设计分析

## 1. 执行摘要

本文档深入分析 **SUBGRAPH（方案 C：独立执行实体）** 和 **EmbedGraph（轻量级展开）** 两种子工作流复用机制的设计方案。

**核心结论**：
- ✅ **SUBGRAPH 方案 C 设计合理**，与现有 Fork/Triggered 模式一致
- ✅ **EmbedGraph 作为未来优化选项可行**，但需严格限制使用场景
- ⚠️ **实施风险可控**，但需要系统性地弃用图展开模型
- 📋 **建议分阶段实施**，先完成 SUBGRAPH 重构，再评估 EmbedGraph 必要性

---

## 2. 背景与问题定义

### 2.1 当前架构的问题

#### 现状：图展开模型（Graph Expansion）
```typescript
// Build Time: SUBGRAPH 节点被展开
Parent Workflow: [START] → [SUBGRAPH:child-wf] → [END]

Expanded Graph:
[START] → [SUBGRAPH_START:child.start] → [child.node1] → 
[child.node2] → [SUBGRAPH_END:child.end] → [END]
```

**问题**：
1. ❌ **变量传递断裂**：展开后所有节点在同一执行实体中，无法区分父子作用域
2. ❌ **SUBGRAPH_START/END 处理器缺失**：目前没有专用处理器处理变量导入/导出
3. ❌ **架构不一致**：Fork/Triggered 使用独立实体，Subgraph 却用展开模型
4. ❌ **作用域清理困难**：无法在退出时自动清理子图局部变量

---

### 2.2 设计目标

基于用户的核心需求：
> "任何边界显式定义映射，保证内部仅能使用所有传递进来的变量，离开作用域时再以显式映射的方式传递回去"

我们需要：
1. **清晰的隔离边界**：子工作流只能访问显式声明的变量
2. **值传递语义**：跨边界的变量传递使用深拷贝
3. **双向映射**：输入和输出都需要显式声明
4. **自动清理**：退出子图时自动清理局部状态
5. **架构一致性**：与 Fork/Triggered 模式统一

---

## 3. SUBGRAPH 方案 C：独立执行实体

### 3.1 核心设计

#### 架构对比

| 维度 | 当前（图展开） | 方案 C（独立实体） |
|------|---------------|-------------------|
| **执行实体** | 单一实体 | 父实体 + 子实体 |
| **变量管理** | 共享 VariableManager | 独立的 VariableManager |
| **作用域隔离** | ❌ 无隔离 | ✅ 完全隔离 |
| **变量传递** | ❌ 隐式继承 | ✅ 显式映射 |
| **生命周期** | 同步执行 | 仅同步执行（异步应使用 FORK） |
| **架构一致性** | ❌ 独特模式 | ✅ 与 Fork/Triggered 一致 |

#### 执行流程

```typescript
// NodeExecutionCoordinator 检测到 SUBGRAPH 节点
if (node.type === 'SUBGRAPH') {
  // 1. 创建独立的子执行实体
  const childEntity = await executionBuilder.createSubgraph(
    parentEntity,
    node.config.subgraphId,
    {
      inputs: node.config.variableInputs,   // 显式输入映射
      outputs: node.config.variableOutputs  // 显式输出映射
    }
  );
  
  // 2. 注册父子关系
  parentEntity.registerChild({
    childType: 'WORKFLOW',
    childId: childEntity.id,
    nodeId: node.id
  });
  
  // 3. 执行子工作流（同步执行）
  await executor.executeWorkflowExecution(childEntity);
  
  // 4. 导出变量回父工作流
  parentEntity.variableStateManager.exportVariables(
    childEntity.variableStateManager,
    node.config.variableOutputs
  );
}
```

---

### 3.2 API 设计

#### `createSubgraph()` 签名

```typescript
interface SubgraphCreationOptions {
  /** 子工作流 ID */
  subworkflowId: string;
  
  /** 变量映射配置 */
  variableMapping?: {
    /** 输入映射：父 → 子 */
    inputs?: WorkflowVariableInput[];
    
    /** 输出映射：子 → 父 */
    outputs?: WorkflowVariableOutput[];
  };
  
  /** 消息上下文映射（可选） */
  messageContextMapping?: Record<string, string>;
  
  /**
   * NOTE: SUBGRAPH only supports synchronous execution.
   * For asynchronous/parallel execution, use FORK nodes instead.
   */
  async?: false; // Always false for SUBGRAPH
}

class WorkflowExecutionBuilder {
  /**
   * 创建子工作流执行实体
   * 
   * @param parentEntity 父工作流执行实体
   * @param options 子工作流创建选项
   * @returns 包含子实体、状态协调器、会话管理器的构建结果
   */
  async createSubgraph(
    parentEntity: WorkflowExecutionEntity,
    options: SubgraphCreationOptions
  ): Promise<WorkflowExecutionBuildResult> {
    // 实现细节见下文
  }
}
```

#### 实现要点

```typescript
async createSubgraph(
  parentEntity: WorkflowExecutionEntity,
  options: SubgraphCreationOptions
): Promise<WorkflowExecutionBuildResult> {
  const parentExecution = parentEntity.getWorkflowExecutionData();
  const subgraphExecutionId = generateId();
  
  logger.info("Creating subgraph execution", {
    parentExecutionId: parentEntity.id,
    subgraphExecutionId,
    subworkflowId: options.subworkflowId,
  });
  
  // 1. 获取子工作流图（已预处理）
  const subgraphGraph = this.getWorkflowGraphRegistry().get(options.subworkflowId);
  if (!subgraphGraph) {
    throw new ExecutionError(`Subworkflow '${options.subworkflowId}' not found`);
  }
  
  // 2. 创建子工作流执行数据
  const subgraphExecution: WorkflowExecution = {
    id: subgraphExecutionId,
    workflowId: options.subworkflowId,
    workflowVersion: subgraphGraph.workflowVersion,
    currentNodeId: subgraphGraph.getStartNodeId(),
    graph: subgraphGraph,
    variables: [], // 初始为空，通过 importVariables 填充
    variableScopes: {
      global: parentExecution.variableScopes.global, // 共享全局作用域
      execution: {}, // 空执行作用域
    },
    input: {}, // 将通过 variableInputs 填充
    output: {},
    nodeResults: [],
    errors: [],
    executionType: "SUBGRAPH",
    hierarchy: {
      parent: {
        parentType: 'WORKFLOW',
        parentId: parentEntity.id,
        nodeId: options.nodeId, // SUBGRAPH 节点 ID
      },
      children: [],
      depth: parentEntity.getHierarchyMetadata()?.depth ? 
             parentEntity.getHierarchyMetadata()!.depth + 1 : 1,
      rootExecutionId: parentEntity.getRootExecutionId(),
      rootExecutionType: parentEntity.getRootExecutionType(),
    },
  };
  
  // 3. 创建执行实体
  const executionState = new ExecutionState();
  const workflowExecutionState = new WorkflowExecutionState();
  const registry = this.getExecutionHierarchyRegistry();
  
  const subgraphEntity = new WorkflowExecutionEntity(
    subgraphExecution,
    executionState,
    workflowExecutionState,
    undefined,
    registry
  );
  
  // 4. 导入变量（使用显式映射 + 深拷贝）
  if (options.variableMapping?.inputs && options.variableMapping.inputs.length > 0) {
    subgraphEntity.variableStateManager.importVariables(
      parentEntity.variableStateManager,
      options.variableMapping.inputs
    );
  }
  
  // 5. 创建会话管理器（克隆父会话）
  const conversationManager = new ConversationSession({
    eventManager: this.getEventManager(),
    workflowExecutionId: subgraphExecutionId,
    workflowId: options.subworkflowId,
    initialMessages: parentEntity.messageHistoryManager.getMessages(),
  });
  
  // 6. 创建状态协调器
  const stateCoordinator = new WorkflowStateCoordinator({
    workflowExecutionEntity: subgraphEntity,
    conversationManager,
  });
  
  // 7. 注册到层级注册表
  registry.register(subgraphEntity);
  parentEntity.registerChild({
    childType: 'WORKFLOW',
    childId: subgraphExecutionId,
    nodeId: options.nodeId,
    spawnedAt: Date.now(),
  });
  
  logger.debug("Subgraph execution created", {
    subgraphExecutionId,
    parentExecutionId: parentEntity.id,
    variableInputCount: options.variableMapping?.inputs?.length || 0,
  });
  
  return {
    workflowExecutionEntity: subgraphEntity,
    stateCoordinator,
    conversationManager,
  };
}
```

---

### 3.3 变量传递机制

#### 输入映射（Parent → Child）

```typescript
// 父工作流配置
{
  "nodes": [
    {
      "id": "process_data",
      "type": "SUBGRAPH",
      "config": {
        "subgraphId": "data-processor",
        "variableInputs": [
          {
            "externalName": "user_id",      // 父工作流中的变量名
            "internalName": "uid",           // 子工作流中的变量名
            "required": true,
            "description": "User identifier"
          },
          {
            "externalName": "config",
            "internalName": "settings",
            "defaultValue": { timeout: 5000 }
          }
        ]
      }
    }
  ]
}

// 运行时行为
parent.variables = { user_id: "123", config: { timeout: 3000 } }

// importVariables 执行后
child.variables = { uid: "123", settings: { timeout: 3000 } }
// ⚠️ 注意：使用的是深拷贝，修改 settings 不会影响 parent.config
```

#### 输出映射（Child → Parent）

```typescript
// 子工作流配置
{
  "nodes": [
    {
      "id": "end",
      "type": "END",
      "config": {
        "variableOutputs": [
          {
            "internalName": "result",        // 子工作流中的变量名
            "externalName": "processed_data", // 父工作流中的变量名
            "description": "Processed result"
          }
        ]
      }
    }
  ]
}

// 运行时行为
child.variables = { result: { status: "ok", data: [...] } }

// exportVariables 执行后
parent.variables.processed_data = { status: "ok", data: [...] }
// ⚠️ 注意：同样使用深拷贝
```

---

### 3.4 优势分析

#### ✅ 1. 清晰的作用域隔离

```typescript
// 父工作流
parent.setVariable("secret_key", "abc123");
parent.setVariable("user_id", "user-456");

// 子工作流只接收显式声明的变量
createSubgraph(parent, {
  variableMapping: {
    inputs: [
      { externalName: "user_id", internalName: "uid" }
      // secret_key 未声明，子工作流无法访问
    ]
  }
});

// 子工作流内部
child.getVariable("uid");        // ✅ "user-456"
child.getVariable("secret_key"); // ❌ undefined（甚至不知道这个变量存在）
```

**收益**：
- 防止意外的状态泄漏
- 明确的接口契约
- 更容易理解和调试

---

#### ✅ 2. 自动清理

```typescript
// 子工作流执行完成后
await executor.executeWorkflowExecution(childEntity);

// 自动清理
childEntity.cleanup(); // 释放资源
registry.unregister(childEntity.id); // 从注册表移除

// 父工作流不受影响
parent.getVariable("user_id"); // ✅ 仍然可用
```

**收益**：
- 无内存泄漏风险
- 无需手动管理生命周期
- 支持嵌套子图（每个都有独立生命周期）

---

#### ✅ 3. 与 Fork/Triggered 一致

| 特性 | Fork | Triggered Subworkflow | SUBGRAPH (方案 C) |
|------|------|----------------------|-------------------|
| 独立实体 | ✅ | ✅ | ✅ |
| 深拷贝变量 | ✅ | ✅ | ✅ |
| 层级注册 | ✅ | ✅ | ✅ |
| 事件系统 | ✅ | ✅ | ✅ |

**收益**：
- 统一的编程模型
- 减少学习成本
- 代码复用率高

---

#### ✅ 4. 支持并行执行多个子图

```typescript
// SUBGRAPH 仅支持同步执行
const result = await executor.executeWorkflowExecution(childEntity);

// 如需并行执行，使用 FORK 节点
// Fork 会自动创建多个独立的分支并行执行
```

**注意**：SUBGRAPH 设计为同步执行，用于嵌入工作流内部。如果需要并行/异步执行，应该使用 FORK 节点。

---

### 3.5 潜在问题与解决方案

#### ⚠️ 问题 1：性能开销

**担忧**：创建独立实体比图展开更重

**分析**：
```typescript
// 图展开：轻量
- 节点合并：O(n) 时间复杂度
- 无额外对象创建

// 独立实体：较重
- 创建 WorkflowExecution 对象
- 创建 VariableManager
- 创建 ConversationSession
- 深拷贝变量
```

**解决方案**：
1. **对象池化**：复用执行实体对象
2. **懒加载**：仅在需要时创建会话管理器
3. **选择性深拷贝**：对大对象使用引用计数
4. **性能监控**：实际测量后再优化

**结论**：在大多数场景下，性能差异可忽略。仅在高频调用的小子图中可能成为瓶颈 → 这正是 EmbedGraph 的用武之地。

---

#### ⚠️ 问题 2：迁移成本

**担忧**：现有工作流依赖图展开模型

**影响范围**：
- `WorkflowGraphBuilder.mergeGraph()` - 需要标记为 @deprecated
- `NodeExecutionCoordinator` - 需要更新 SUBGRAPH 处理逻辑
- 测试用例 - 需要更新以反映新行为

**迁移策略**：
```typescript
// Phase 1: 双轨运行
if (featureFlags.useNewSubgraphModel) {
  // 新模型：独立实体
  await handleSubgraphWithSeparateEntity(node);
} else {
  // 旧模型：图展开（向后兼容）
  await handleSubgraphWithExpansion(node);
}

// Phase 2: 警告期
logger.warn("Graph expansion model is deprecated. Use separate entity model.");

// Phase 3: 移除旧模型
// 删除 mergeGraph() 和相关代码
```

**结论**：迁移成本可控，可通过渐进式迁移降低风险。

---

#### ⚠️ 问题 3：嵌套子图的复杂性

**场景**：
```
Parent Workflow
  └─ SUBGRAPH A (creates Entity A)
      └─ SUBGRAPH B (creates Entity B)
          └─ SUBGRAPH C (creates Entity C)
```

**挑战**：
- 层级管理复杂
- 变量传递链长
- 错误传播路径深

**解决方案**：
```typescript
// 利用现有的 ExecutionHierarchyManager
parentEntity.registerChild({ childType: 'WORKFLOW', childId: entityA.id });
entityA.registerChild({ childType: 'WORKFLOW', childId: entityB.id });
entityB.registerChild({ childType: 'WORKFLOW', childId: entityC.id });

// 查询整个层级
const allDescendants = registry.getAllDescendants(parentEntity.id);
// [entityA, entityB, entityC, ...]

// 清理整个子树
registry.cleanupHierarchy(parentEntity.id);
// 自动清理 A, B, C 及所有后代
```

**结论**：现有层级管理系统已支持嵌套结构，无需额外开发。

---

## 4. EmbedGraph：轻量级展开优化

### 4.1 设计动机

**问题**：并非所有子图都需要完整的隔离

**场景分类**：

| 场景 | 需要隔离？ | 推荐方案 |
|------|-----------|---------|
| 包含变量操作 | ✅ 是 | SUBGRAPH（方案 C） |
| 包含 trigger | ✅ 是 | SUBGRAPH（方案 C） |
| 纯控制流复用 | ❌ 否 | EmbedGraph |
| 高频调用的小子图 | ❌ 否 | EmbedGraph |

**示例**：
```toml
# 场景：错误处理模板（无变量，纯控制流）
[workflow]
id = "error-handler-template"
type = "DEPENDENT"

[[nodes]]
id = "check_error"
type = "ROUTE"
config = { conditions = [...] }

[[nodes]]
id = "log_error"
type = "SCRIPT"
config = { scriptId = "log-error-script" }

[[edges]]
from = "check_error"
to = "log_error"
```

这种子图：
- ❌ 没有变量定义
- ❌ 没有 trigger
- ❌ 没有 VARIABLE 节点
- ✅ 只是控制流模板

→ 适合 EmbedGraph

---

### 4.2 设计规范

#### 类型定义

```typescript
// 新增节点类型
type StaticNodeType = 
  | "START" | "END" | "FORK" | "JOIN"
  | "SUBGRAPH"      // 完整隔离，独立实体
  | "EMBED_GRAPH"   // 轻量展开，无隔离
  | ...;

interface EmbedGraphNodeConfig {
  /** 嵌入的工作流 ID */
  embedId: ID;
  
  /** 
   * 注意：EmbedGraph 不允许以下配置
   * - variableInputs/Outputs
   * - messageContextMapping
   * - async 选项
   */
}
```

---

#### 静态验证规则

```typescript
class WorkflowGraphValidator {
  /**
   * 验证 EmbedGraph 节点的约束
   */
  validateEmbedGraph(
    embedNode: StaticNode,
    embeddedWorkflow: WorkflowTemplate
  ): ValidationResult {
    const errors: string[] = [];
    
    // 规则 1: 嵌入的工作流不能定义变量
    if (embeddedWorkflow.variables && embeddedWorkflow.variables.length > 0) {
      errors.push(
        `EMBED_GRAPH '${embedNode.id}' references workflow '${embeddedWorkflow.id}' ` +
        `which defines ${embeddedWorkflow.variables.length} variables. ` +
        `EmbedGraph workflows must be variable-free.`
      );
    }
    
    // 规则 2: 嵌入的工作流不能有 trigger
    if (embeddedWorkflow.triggers && embeddedWorkflow.triggers.length > 0) {
      errors.push(
        `EMBED_GRAPH '${embedNode.id}' references workflow '${embeddedWorkflow.id}' ` +
        `which defines ${embeddedWorkflow.triggers.length} triggers. ` +
        `EmbedGraph workflows cannot have triggers.`
      );
    }
    
    // 规则 3: 嵌入的工作流不能包含 VARIABLE 节点
    const hasVariableNodes = embeddedWorkflow.nodes.some(
      node => node.type === 'VARIABLE'
    );
    if (hasVariableNodes) {
      errors.push(
        `EMBED_GRAPH '${embedNode.id}' references workflow '${embeddedWorkflow.id}' ` +
        `which contains VARIABLE nodes. EmbedGraph workflows cannot modify variables.`
      );
    }
    
    // 规则 4: 递归检查嵌套的 SUBGRAPH/EMBED_GRAPH
    for (const node of embeddedWorkflow.nodes) {
      if (node.type === 'SUBGRAPH' || node.type === 'EMBED_GRAPH') {
        const nestedWorkflow = this.getWorkflow(node.config.embedId || node.config.subgraphId);
        const nestedResult = this.validateEmbedGraph(node, nestedWorkflow);
        errors.push(...nestedResult.errors);
      }
    }
    
    return errors.length > 0 
      ? { valid: false, errors }
      : { valid: true, errors: [] };
  }
}
```

---

#### 运行时行为

```typescript
// NodeExecutionCoordinator 处理 EMBED_GRAPH
if (node.type === 'EMBED_GRAPH') {
  // 直接使用图展开（当前行为）
  // 因为已验证无变量/trigger，所以安全
  
  const embeddedGraph = this.getWorkflowGraphRegistry().get(node.config.embedId);
  
  // 展开并合并到当前图
  this.mergeGraph(embeddedGraph, node.id);
  
  // 继续执行（在同一实体中）
  // 无需创建新实体，无需变量传递
}
```

---

### 4.3 优势与限制

#### ✅ 优势

1. **零性能开销**
   - 无额外对象创建
   - 无深拷贝操作
   - 纯粹的节点合并

2. **简化高频场景**
   ```typescript
   // 场景：循环中调用小子图
   for (let i = 0; i < 1000; i++) {
     // EmbedGraph: 快速展开，无开销
     // SUBGRAPH: 每次创建新实体，有开销
   }
   ```

3. **明确的语义**
   - `SUBGRAPH` = "我需要隔离"
   - `EMBED_GRAPH` = "我只是复用控制流"

---

#### ⚠️ 限制

1. **严格的约束**
   - 不能有任何变量操作
   - 不能有 trigger
   - 不能嵌套使用变量的子图

2. **适用场景有限**
   - 仅适合纯控制流模板
   - 大多数实际子图需要变量 → 必须用 SUBGRAPH

3. **增加复杂度**
   - 开发者需要理解两种模式的区别
   - 静态验证增加构建时间
   - 文档和教育成本

---

### 4.4 决策指南

```
是否需要变量隔离？
├─ 是 → 使用 SUBGRAPH
│   ├─ 需要异步执行？ → SUBGRAPH with async=true
│   └─ 需要同步执行？ → SUBGRAPH with async=false
│
└─ 否 → 检查是否满足 EmbedGraph 约束
    ├─ 满足 → 可以使用 EMBED_GRAPH（性能优化）
    └─ 不满足 → 必须使用 SUBGRAPH
```

**推荐策略**：
- **默认使用 SUBGRAPH**（安全、清晰）
- **仅在性能分析显示瓶颈时**考虑 EmbedGraph
- **不要过早优化**

---

## 5. 综合对比

### 5.1 功能对比表

| 特性 | SUBGRAPH (方案 C) | EmbedGraph | 图展开（当前） |
|------|------------------|------------|---------------|
| **作用域隔离** | ✅ 完全隔离 | ❌ 无隔离 | ❌ 无隔离 |
| **变量传递** | ✅ 显式映射 | ❌ 不支持 | ❌ 隐式继承 |
| **深拷贝** | ✅ 自动 | N/A | ❌ 无 |
| **执行模式** | ✅ 仅同步 | ❌ 仅同步 | ❌ 仅同步 |
| **触发器支持** | ✅ 支持 | ❌ 禁止 | ✅ 支持 |
| **性能开销** | 🟡 中等 | ✅ 零开销 | ✅ 低开销 |
| **架构一致性** | ✅ 与 Fork 一致 | ⚠️ 独特 | ❌ 独特 |
| **静态验证** | 🟡 可选 | ✅ 强制 | ❌ 无 |
| **适用场景** | 通用嵌入 | 纯控制流复用 | 通用（但有缺陷） |

---

### 5.2 实施优先级

| 阶段 | 任务 | 优先级 | 预计工作量 |
|------|------|--------|-----------|
| **Phase 1** | SUBGRAPH 方案 C 实现 | 🔴 高 | 5-7 天 |
| **Phase 2** | 弃用图展开模型 | 🟡 中 | 2-3 天 |
| **Phase 3** | 性能分析与基准测试 | 🟢 低 | 1-2 天 |
| **Phase 4** | EmbedGraph 实现（如需） | 🔵 未来 | 3-4 天 |

**建议**：
1. 先完成 Phase 1-2，建立稳定的 SUBARCH 机制
2. 收集实际使用数据和性能指标
3. 基于数据决定是否实施 Phase 4

---

## 6. 风险评估

### 6.1 技术风险

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| 性能下降 | 🟡 中 | 🟡 中 | 性能基准测试，必要时引入 EmbedGraph |
| 迁移困难 | 🟢 低 | 🟡 中 | 渐进式迁移，双轨运行期 |
| 嵌套复杂性 | 🟢 低 | 🟢 低 | 利用现有层级管理系统 |
| 内存泄漏 | 🟢 低 | 🔴 高 | 自动化清理机制，单元测试覆盖 |

**总体技术风险**：🟢 **低**

---

### 6.2 业务风险

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| 破坏现有工作流 | 🟢 低 | 🔴 高 | 向后兼容层，充分的集成测试 |
| 学习曲线陡峭 | 🟡 中 | 🟡 中 | 详细文档，示例代码，迁移指南 |
| 开发周期延长 | 🟡 中 | 🟡 中 | 分阶段实施，优先核心功能 |

**总体业务风险**：🟡 **中低**

---

## 7. 实施路线图

### 7.1 Phase 0: 准备阶段（1-2 天）

**目标**：充分理解当前实现，设计新 API

**任务**：
- [ ] 文档化当前图展开逻辑的所有位置
- [ ] 识别依赖展开的代码路径
- [ ] 设计 `createSubgraph()` API 签名
- [ ] 编写技术方案评审文档
- [ ] 确定测试策略

**交付物**：
- 当前实现分析报告
- API 设计文档
- 测试计划

---

### 7.2 Phase 1: 核心实现（5-7 天）

**目标**：实现 SUBGRAPH 方案 C

**任务**：
- [ ] 实现 `WorkflowExecutionBuilder.createSubgraph()`
- [ ] 更新 `NodeExecutionCoordinator` 处理 SUBGRAPH 节点
- [ ] 集成变量导入/导出逻辑
- [ ] 实现异步执行支持
- [ ] 添加单元测试

**关键文件**：
- `sdk/workflow/execution/factories/workflow-execution-builder.ts`
- `sdk/workflow/execution/coordinators/node-execution-coordinator.ts`
- `sdk/workflow/execution/handlers/subgraph-handler.ts`（可能需要简化）

**验收标准**：
- ✅ 子图可以成功创建独立实体
- ✅ 变量正确导入/导出（深拷贝）
- ✅ 父子关系正确注册
- ✅ SUBGRAPH 仅支持同步执行
- ✅ 单元测试通过率 100%

---

### 7.3 Phase 2: 迁移与弃用（2-3 天）

**目标**：逐步淘汰图展开模型

**任务**：
- [ ] 标记 `mergeGraph()` 为 @deprecated
- [ ] 添加运行时警告
- [ ] 更新文档说明新模型
- [ ] 迁移现有测试用例
- [ ] 编写迁移指南

**关键文件**：
- `sdk/workflow/builder/workflow-graph-builder.ts`
- 所有相关文档

**验收标准**：
- ✅ 旧模型仍有警告但可用
- ✅ 新模型文档完整
- ✅ 迁移指南清晰

---

### 7.5 Phase 3: EmbedGraph（3-4 天）

**任务**：
- [ ] 添加 EMBED_GRAPH 节点类型
- [ ] 实现静态验证器
- [ ] 更新图构建器支持两种模式
- [ ] 编写使用指南
- [ ] 添加示例

**验收标准**：
- ✅ 静态验证正确拒绝非法使用
- ✅ 性能提升可测量
- ✅ 文档清晰说明何时使用哪种模式

---

## 8. 结论与建议

### 8.1 设计合理性评估

#### ✅ SUBGRAPH 方案 C：**设计合理，推荐实施**

**理由**：
1. **架构一致性**：与 Fork/Triggered 模式统一，降低认知负担
2. **清晰的隔离**：显式映射 + 深拷贝，符合最小权限原则
3. **职责明确**：SUBGRAPH 专注于同步嵌入执行，FORK 负责并行/异步
4. **风险可控**：技术风险低，迁移路径清晰
5. **长期价值**：为未来的高级特性（如分布式执行）奠定基础

**潜在问题**：
- 性能开销（可通过 EmbedGraph 优化）
- 迁移成本（可通过渐进式迁移降低）

---

#### ⚠️ EmbedGraph：**可行但需谨慎**

**理由**：
1. **性能优势明显**：零开销，适合高频场景
2. **语义清晰**：明确区分"隔离"vs"复用"
3. **但增加复杂度**：两种模式需要学习和维护

## 9. 术语表

| 术语 | 定义 |
|------|------|
| **SUBGRAPH** | 子工作流节点，创建独立执行实体 |
| **EmbedGraph** | 嵌入式子图，轻量级展开（未来） |
| **图展开** | 将 SUBGRAPH 节点扩展为多个节点的当前模型(按照新的方案，subgraph不再展开，而是作为单独实例，embed才展开) |
| **方案 C** | 使用独立执行实体的 SUBGRAPH 设计方案 |
| **显式映射** | 通过 variableInputs/Outputs 声明变量传递 |
| **深拷贝** | 使用 structuredClone() 复制变量值 |
| **作用域隔离** | 子工作流只能访问显式声明的变量 |
