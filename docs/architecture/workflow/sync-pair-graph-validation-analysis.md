# Paired SYNC 图验证分析

## 1. 背景

基于 Paired SYNC 设计，主工作流和子工作流同时提供 SYNC 节点，通过 `pairId` 一一配对实现双向数据同步。当前图验证系统需要在多个层面补充配对校验逻辑。

## 2. 现有验证体系概览

### 2.1 验证层次

```
WorkflowDefinition (工作流注册)
    ↓ WorkflowValidator.validate() — 静态数据完整性
    ↓
GraphBuilder.buildAndValidate()
    ├── GraphBuilder.build() — 构建单图
    ├── GraphBuilder.processSubgraphs() — 递归合并子图
    └── GraphValidator.validate() — 图拓扑验证 (单图)
    ↓
WorkflowProcessor.preprocessWorkflow() — 综合预处理
```

### 2.2 当前 sync-node-validator.ts 验证能力

| 验证项 | 范围 | 错误码 |
|--------|------|--------|
| sourcePathId 存在性 | 单图 | `MISSING_SYNC_SOURCE_PATH_ID` |
| sourcePathId 合法性 | 单图（FORK pathId） | `INVALID_SYNC_SOURCE_PATH_ID` |
| targetPathId 合法性 | 单图（FORK pathId） | `INVALID_SYNC_TARGET_PATH_ID` |
| variableMapping 字段完整性 | 单图 | `MISSING_SYNC_MAPPING_*` |
| variableMapping 重复检测 | 单图 | `DUPLICATE_SYNC_MAPPING_*` |
| 孤立节点检测 | 单图 | `ISOLATED_SYNC_NODE` |
| 多节点映射冲突 | 单图 | `CONFLICTING_SYNC_MAPPINGS` |
| 循环变量依赖 | 单图 | `CIRCULAR_SYNC_DEPENDENCY` |
| 循环数据流 | 单图 | `CIRCULAR_DATA_FLOW` |

**缺失能力**：
- 无 `pairId` 字段支持
- 无跨图（主工作流 ↔ 子工作流）配对验证
- 无执行上下文兼容性验证
- 无 `variableExchanges` 双向交换字段支持

## 3. Paired SYNC 的验证维度

### 3.1 类型定义层

需要补充的字段：

```typescript
export interface SyncNodeConfig {
  // ... 现有字段 ...

  /**
   * 配对 ID — 标识主工作流/子工作流中配对的 SYNC 节点
   * - 同一 pairId 必须在主工作流和子工作流中各出现一次
   * - 无 pairId 时走原有的 FORK-JOIN 分支同步逻辑
   * - 格式约定: `${mainWorkflowId}:${subWorkflowId}:<name>`
   */
  pairId?: string;

  /**
   * 变量交换配置（支持双向多源多目标）
   * 当 paired SYNC 需要双向数据交换时，替代单向 variableMappings
   * 每个交换条目定义 source 和 target 的完整变量名映射
   */
  variableExchanges?: Array<{
    /** 源方 Path ID（在主工作流或子工作流中的 fork pathId） */
    sourcePathId: ID;
    /** 源方变量名 */
    sourceVariable: string;
    /** 目标方 Path ID */
    targetPathId: ID;
    /** 目标方变量名 */
    targetVariable: string;
  }>;
}
```

### 3.2 Zod Schema 验证层

需要补充的字段：

```typescript
export const SyncNodeConfigSchema = z.object({
  pairId: z.string().min(1, "Pair ID is required for paired SYNC").optional(),
  variableExchanges: z.array(z.object({
    sourcePathId: z.string().min(1),
    sourceVariable: z.string().min(1),
    targetPathId: z.string().min(1),
    targetVariable: z.string().min(1),
  })).optional(),
  // ... 现有字段保持 ...
});
```

### 3.3 图内验证层（Intra-Graph Validation）

在 `sync-node-validator.ts` 中补充的规则：

#### 规则 1：pairId 格式与唯一性

- 注：pairId 非空时必须符合命名规范
- **错误码**: `INVALID_PAIR_ID_FORMAT`
- 注：同一 graph 内 pairId 必须唯一（不能有两个 SYNC 节点声明相同 pairId）
- **错误码**: `DUPLICATE_PAIR_ID_IN_GRAPH`

#### 规则 2：pairId 与 variableExchanges 的一致性

- pairId 存在时，variableExchanges 或 variableMappings 至少有一项
- **错误码**: `PAIRED_SYNC_NO_EXCHANGE`
- variableExchanges 中的 sourcePathId 必须在当前图所属的 FORK 节点的 forkPaths 中
- **错误码**: `INVALID_EXCHANGE_SOURCE_PATH_ID`
- variableExchanges 中的 targetPathId 也必须在当前图所属的 FORK 节点的 forkPaths 中
- **错误码**: `INVALID_EXCHANGE_TARGET_PATH_ID`

#### 规则 3：variableExchanges 自身完整性

- 同一个 exchange 条目中 sourcePathId 和 targetPathId 不能相同
- **错误码**: `SELF_REFERENCE_EXCHANGE`
- 同一 SYNC 节点的 variableExchanges 中不能有重复的 (sourcePathId, sourceVariable) 对
- **错误码**: `DUPLICATE_EXCHANGE_SOURCE`

#### 规则 4：pairId 与 sourcePathId/targetPathId 的共存约束

- pairId 存在时，允许不设置 sourcePathId/targetPathId（交由配对逻辑决定方向）
- pairId 不存在时，必须设置 sourcePathId（保持当前强制规则）

#### 规则 5：拓扑位置约束

- 注：声明 pairId 的 SYNC 节点必须位于 SUBGRAPH 节点的**直接前置或直接后置**路径上
- 错误码: `PAIRED_SYNC_INVALID_POSITION`
- 具体指：该 SYNC 节点必须在图中与 SUBGRAPH 节点有直接边连接，或者位于 SUBGRAPH 节点所在的 FORK-JOIN 分支内
- 对于主工作流侧的 SYNC，其 outgoing edge 应连接到 SUBGRAPH 节点，或从 SUBGRAPH 节点的 upstream 节点可达
- 对于子工作流侧的 SYNC，必须在子工作流的 START-END 之间可达

### 3.4 跨图验证层（Cross-Graph Validation）

这是核心新增的验证层。需要在工作流预处理阶段，当主图和子图都构建完成后执行。

#### 验证时机

```
GraphBuilder.build() (主图)
    ↓
GraphBuilder.processSubgraphs() (构建子图)
    ↓                          ← 在此处插入跨图验证
GraphValidator.validate() (各自独立验证)
    ↓
WorkflowProcessor 收尾
```

实际上两个合适的位置：
1. **在 GraphValidator.validate() 之后**，在 WorkflowProcessor 中新增 `validatePairedSyncCrossGraph()` 调用
2. **在 GraphBuilder.processSubgraphs() 内部**，当子图构建完成且合并前，遍历所有 SUBGRAPH 节点时逐对验证

建议采用位置 1，因为：
- 不侵入 GraphBuilder 的核心构建逻辑
- 此时主图和所有子图都已完成构建和单图验证
- 可以通过 WorkflowRegistry 访问所有已注册的工作流定义和图数据

#### 跨图验证规则

##### 规则 6：配对对称性

扫描所有工作流图中声明了 `pairId` 的 SYNC 节点，对于每个 `pairId`：

- 必须存在恰好 2 个 SYNC 节点声明该 `pairId`
- **错误码**: `PAIR_ID_NOT_SYMMETRIC`
- 其中一个必须在主工作流图中，另一个必须在子工作流图中
- **错误码**: `PAIR_ID_SCOPE_MISMATCH`

具体判定逻辑：
```
1. 收集主工作流 G 中所有含 pairId 的 SYNC 节点 → set P
2. 收集所有子工作流 Gs 中含 pairId 的 SYNC 节点 → set Q
3. 对于 p ∈ P，在 Q 中查找 q 满足 p.pairId === q.pairId
4. 若未找到 → 错误（子工作流缺少配对节点）
5. 若找到多个 → 错误（子工作流有重复配对）
6. 同理验证反向：对于 q ∈ Q，在 P 中查找
```

##### 规则 7：配对连接可达性

对于配对的 SYNC 节点对 (mainSync, subSync)：

- mainSync 必须在主工作流图中可达于某个 SUBGRAPH 节点（通过图遍历）
- subSync 必须在子工作流图的 START-END 路径上
- **错误码**: `PAIRED_SYNC_UNREACHABLE`

##### 规则 8：变量交换一致性

对于配对的 SYNC 节点对，其变量映射必须是**可交换的**：

```
若 mainSync.variableExchanges 中包含:
  { sourcePathId: "pathA", sourceVariable: "x", targetPathId: "pathB", targetVariable: "y" }

则 subSync.variableExchanges 中应包含对应的反向条目（含义由 sync 处理器解释）:
  不一定需要严格的对称，因为主侧和子侧的语义不同。
  但需要保证变量名在各自上下文中都存在。
```

更准确地说，验证规则是：

- 主工作流 SYNC 声明的 `variableExchanges` 中的 sourceVariable 必须在主工作流的变量定义中存在
- 子工作流 SYNC 声明的 `variableExchanges` 中的 sourceVariable 必须在子工作流的变量定义中存在
- 主工作流 SYNC 的 `variableMappings` 中的 externalName 必须在主工作流变量中存在
- 子工作流 SYNC 的 `variableMappings` 中的 externalName 必须在子工作流变量中存在
- **错误码**: `PAIRED_SYNC_VARIABLE_NOT_FOUND`

##### 规则 9：FORK-JOIN 上下文兼容性

如果 paired SYNC 节点位于 FORK-JOIN 分支内：

- 主工作流侧的 SYNC 所在的 FORK pathId 必须在主工作流的 FORK 中定义
- 子工作流侧的 SYNC 所在的 FORK pathId 必须在子工作流的 FORK 中定义
- 两者不必使用相同的 pathId（因为主/子工作流可能使用不同的命名）
- **错误码**: `PAIRED_SYNC_FORK_CONTEXT_INCOMPATIBLE`

#### 跨图验证函数的签名设计

```typescript
/**
 * Validate paired SYNC nodes across main and sub-workflow graphs
 *
 * Cross-graph validation should be called after all graphs are built
 * but before they are cached and committed to execution.
 *
 * @param mainGraph Main workflow's graph
 * @param subworkflowGraphs Map of subworkflow ID → its graph
 * @param workflowRegistry Access to workflow definitions (for variable definitions)
 * @returns List of configuration validation errors
 */
export function validatePairedSyncCrossGraph(
  mainGraph: WorkflowGraphData,
  subworkflowGraphs: Map<ID, WorkflowGraphData>,
  workflowRegistry: WorkflowRegistry,
): ConfigurationValidationError[] {
  // Step 1: Collect all paired SYNC nodes from main graph
  // Step 2: For each SUBGRAPH node in main graph, get its subworkflow graph
  // Step 3: Match pairIds across graphs
  // Step 4: Validate each pair's consistency
  // Step 5: Validate variable existence in respective workflow definitions
  // Step 6: Validate FORK-JOIN context compatibility
}
```

### 3.5 执行上下文验证（Execution Context Validation）

此层验证在运行时（执行前）通过检查 `WorkflowExecutionRegistry` 和 `SyncBarrier` 完成，属于动态验证。

#### 规则 10：配对 SYNC 的执行上下文匹配

- 运行时，主工作流侧的 SYNC 处理器需能找到对应的子执行实例
- 子工作流侧的 SYNC 处理器需能找到对应的父执行实例
- 若执行层次不匹配（如子工作流已销毁），应给出明确错误

此规则在执行时通过现有机制（`WorkflowExecutionRegistry`、`getParentContext()`）即可处理，无需在预处理阶段实现。

## 4. 验证优先级与严重程度

| 规则 | 验证层 | 严重程度 | 实现阶段 |
|------|--------|----------|----------|
| R1 pairId 格式与唯一性 | 图内 | 严重 | 阶段 1 |
| R2 pairId 与 exchange 一致性 | 图内 | 严重 | 阶段 1 |
| R3 exchange 自身完整性 | 图内 | 中等 | 阶段 1 |
| R4 pairId 与 sourcePathId 共存 | 图内 | 中等 | 阶段 1 |
| R5 拓扑位置约束 | 图内 | 严重 | 阶段 1 |
| R6 配对对称性 | 跨图 | 严重 | 阶段 2 |
| R7 配对连接可达性 | 跨图 | 严重 | 阶段 2 |
| R8 变量交换一致性 | 跨图 | 严重 | 阶段 2 |
| R9 FORK-JOIN 上下文兼容性 | 跨图 | 中等 | 阶段 2 |
| R10 执行上下文匹配 | 运行时 | 严重 | 现有机制 |

## 5. 具体实现设计

### 5.1 类型定义变更

[SyncNodeConfig](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/sync-configs.ts) 新增字段：

```typescript
export interface SyncNodeConfig {
  // ... 现有字段保持不变 ...
  sourcePathId: ID;
  targetPathId?: ID;
  variableMappings?: WorkflowVariableInput[];
  dataInputs?: WorkflowDataInput[];
  messageInputs?: WorkflowMessageInput[];
  waitForCompletion?: boolean;
  timeout?: number;

  // === 新增字段 ===

  /** 配对 ID — 标识主工作流/子工作流中配对的 SYNC 节点 */
  pairId?: string;

  /** 变量交换配置 — 支持双向多源多目标交换 */
  variableExchanges?: SyncVariableExchange[];
}

/** 变量交换条目 */
export interface SyncVariableExchange {
  /** 源方 Path ID */
  sourcePathId: ID;
  /** 源方变量名 */
  sourceVariable: string;
  /** 目标方 Path ID */
  targetPathId: ID;
  /** 目标方变量名 */
  targetVariable: string;
}
```

### 5.2 Zod Schema 变更

[sync-configs-schema.ts](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/sync-configs-schema.ts) 新增：

```typescript
const SyncVariableExchangeSchema = z.object({
  sourcePathId: z.string().min(1, "Exchange source path ID is required"),
  sourceVariable: z.string().min(1, "Exchange source variable is required"),
  targetPathId: z.string().min(1, "Exchange target path ID is required"),
  targetVariable: z.string().min(1, "Exchange target variable is required"),
});

export const SyncNodeConfigSchema = z.object({
  // ... 现有字段 ...
  pairId: z.string().min(1).optional(),
  variableExchanges: z.array(SyncVariableExchangeSchema).optional(),
  // ... 其余现有字段 ...
});
```

### 5.3 图内验证增强

在 [sync-node-validator.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/validation/graph-validation/sync-node-validator.ts) 中：

```typescript
export function validateSyncNodes(graph: WorkflowGraphData): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];
  const syncNodes = collectSyncNodes(graph);
  if (syncNodes.length === 0) return errors;

  // 1. 现有验证保持不变
  validateSourcePathId(syncNodes, forkPathMapping, errors);
  validateTargetPathId(syncNodes, forkPathMapping, errors);
  validateVariableMappings(syncNodes, errors);
  validateIsolation(syncNodes, graph, errors);

  // 2. 新增图内验证
  validatePairIdFormat(syncNodes, errors);          // R1
  validatePairIdUniqueness(syncNodes, errors);       // R1
  validatePairIdConsistency(syncNodes, errors);       // R2
  validateExchangeIntegrity(syncNodes, errors);       // R3
  validatePairIdCoexistence(syncNodes, errors);       // R4
  validateTopologicalPosition(syncNodes, graph, errors); // R5

  validateSyncNodePairing(syncNodes, forkPathMapping, errors);
  validateDataFlowDirection(syncNodes, errors);

  return errors;
}

function validatePairIdFormat(syncNodes, errors): void {
  for (const { nodeId, config } of syncNodes) {
    if (config.pairId && !/^[\w-]+:[\w-]+:[\w.-]+$/.test(config.pairId)) {
      errors.push(new ConfigurationValidationError(
        `SYNC node '${nodeId}' has invalid pairId format '${config.pairId}'. Expected format: 'mainWfId:subWfId:name'`,
        { configType: "workflow", context: { code: "INVALID_PAIR_ID_FORMAT", nodeId, pairId: config.pairId } }
      ));
    }
  }
}

function validatePairIdUniqueness(syncNodes, errors): void {
  const pairIdMap = new Map<string, string[]>(); // pairId -> [nodeIds]
  for (const { nodeId, config } of syncNodes) {
    if (config.pairId) {
      const existing = pairIdMap.get(config.pairId) || [];
      existing.push(nodeId);
      pairIdMap.set(config.pairId, existing);
    }
  }
  for (const [pairId, nodeIds] of pairIdMap) {
    if (nodeIds.length > 1) {
      errors.push(new ConfigurationValidationError(
        `Duplicate pairId '${pairId}' in the same graph: nodes ${nodeIds.join(', ')}. pairId must be unique within a single graph.`,
        { configType: "workflow", context: { code: "DUPLICATE_PAIR_ID_IN_GRAPH", pairId, nodeIds } }
      ));
    }
  }
}

function validatePairIdConsistency(syncNodes, errors): void {
  for (const { nodeId, config } of syncNodes) {
    if (config.pairId) {
      const hasMapping = (config.variableMappings?.length ?? 0) > 0;
      const hasExchange = (config.variableExchanges?.length ?? 0) > 0;
      if (!hasMapping && !hasExchange) {
        errors.push(new ConfigurationValidationError(
          `Paired SYNC node '${nodeId}' (pairId: '${config.pairId}') must have at least one variableMapping or variableExchange`,
          { configType: "workflow", context: { code: "PAIRED_SYNC_NO_EXCHANGE", nodeId, pairId: config.pairId } }
        ));
      }
    }
  }
}

function validateTopologicalPosition(syncNodes, graph, errors): void {
  // Find SUBGRAPH nodes in the graph
  const subgraphNodeIds = new Set<string>();
  for (const node of graph.nodes.values()) {
    if (node.type === "SUBGRAPH") {
      subgraphNodeIds.add(node.id);
    }
  }

  for (const { nodeId, config } of syncNodes) {
    if (!config.pairId) continue;
    if (subgraphNodeIds.size === 0) {
      // SYNC with pairId but no SUBGRAPH in this graph → might be in subworkflow itself
      continue;
    }
    // Check if SYNC node is connected to a SUBGRAPH node (directly or transitively)
    const isConnectedToSubgraph = checkConnectivityToSubgraph(nodeId, subgraphNodeIds, graph);
    if (!isConnectedToSubgraph) {
      errors.push(new ConfigurationValidationError(
        `Paired SYNC node '${nodeId}' (pairId: '${config.pairId}') is not topologically connected to any SUBGRAPH node`,
        { configType: "workflow", context: { code: "PAIRED_SYNC_INVALID_POSITION", nodeId, pairId: config.pairId } }
      ));
    }
  }
}
```

### 5.4 跨图验证器设计

新增验证模块：[sync-pair-cross-graph-validator.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/validation/graph-validation/sync-pair-cross-graph-validator.ts)

```typescript
/**
 * Paired SYNC 跨图验证器
 *
 * 在预处理阶段调用，当主图和子图都已完成构建和单图验证后执行。
 * 验证 pairId 的对称性、变量交换一致性、FORK-JOIN 上下文兼容性。
 */

import type { ID, SyncNodeConfig, ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphData } from "../../entities/workflow-graph-data.js";
import type { WorkflowRegistry } from "../../../core/services/workflow-registry.js";

interface SyncNodeInfo {
  nodeId: ID;
  config: SyncNodeConfig;
  graphId: ID;             // Workflow ID of the graph containing this node
  isMainGraph: boolean;    // Whether it's in the main workflow graph
}

/**
 * Cross-graph paired SYNC validation
 *
 * @param mainGraph Main workflow's graph data
 * @param subworkflowGraphs Map of subworkflow workflow ID → its graph data
 * @param workflowRegistry Workflow registry for accessing variable definitions
 * @returns List of validation errors
 */
export function validatePairedSyncCrossGraph(
  mainGraph: WorkflowGraphData,
  subworkflowGraphs: Map<ID, WorkflowGraphData>,
  workflowRegistry: WorkflowRegistry,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Step 1: Collect all paired SYNC nodes from main graph
  const mainPairedSyncs = collectPairedSyncNodes(mainGraph, mainGraph.workflowId, true);

  // Step 2: Collect all paired SYNC nodes from subworkflow graphs
  const subPairedSyncs: SyncNodeInfo[] = [];
  for (const [subGraphId, subGraph] of subworkflowGraphs) {
    const collected = collectPairedSyncNodes(subGraph, subGraphId, false);
    subPairedSyncs.push(...collected);
  }

  // Step 3: Match pairIds
  const mainPairMap = new Map<ID, SyncNodeInfo>();
  for (const sync of mainPairedSyncs) {
    if (sync.config.pairId) {
      mainPairMap.set(sync.config.pairId, sync);
    }
  }

  const subPairMap = new Map<ID, SyncNodeInfo>();
  for (const sync of subPairedSyncs) {
    if (sync.config.pairId) {
      subPairMap.set(sync.config.pairId, sync);
    }
  }

  // Step 4: Validate symmetry (R6)
  validatePairSymmetry(mainPairMap, subPairMap, errors);

  // Step 5: Validate variable exchange consistency (R8)
  validateExchangeConsistency(mainPairMap, subPairMap, workflowRegistry, errors);

  // Step 6: Validate FORK-JOIN context compatibility (R9)
  validateForkContextCompatibility(mainPairMap, subPairMap, mainGraph, errors);

  return errors;
}

function validatePairSymmetry(
  mainPairMap: Map<ID, SyncNodeInfo>,
  subPairMap: Map<ID, SyncNodeInfo>,
  errors: ConfigurationValidationError[],
): void {
  for (const [pairId, mainSync] of mainPairMap) {
    const subSync = subPairMap.get(pairId);
    if (!subSync) {
      errors.push(new ConfigurationValidationError(
        `Paired SYNC node '${mainSync.nodeId}' (pairId: '${pairId}') in main workflow has no matching paired SYNC in subworkflow graphs`,
        { configType: "workflow", context: { code: "PAIR_ID_NOT_SYMMETRIC", pairId, mainNodeId: mainSync.nodeId } }
      ));
    }
  }

  for (const [pairId, subSync] of subPairMap) {
    if (!mainPairMap.has(pairId)) {
      errors.push(new ConfigurationValidationError(
        `Paired SYNC node '${subSync.nodeId}' (pairId: '${pairId}') in subworkflow has no matching paired SYNC in main workflow`,
        { configType: "workflow", context: { code: "PAIR_ID_NOT_SYMMETRIC", pairId, subNodeId: subSync.nodeId } }
      ));
    }
  }
}

function validateExchangeConsistency(
  mainPairMap: Map<ID, SyncNodeInfo>,
  subPairMap: Map<ID, SyncNodeInfo>,
  workflowRegistry: WorkflowRegistry,
  errors: ConfigurationValidationError[],
): void {
  for (const [pairId, mainSync] of mainPairMap) {
    const subSync = subPairMap.get(pairId);
    if (!subSync) continue;

    // Validate variable existence in respective workflow contexts
    const mainWorkflow = workflowRegistry.get(mainSync.graphId);
    const subWorkflow = workflowRegistry.get(subSync.graphId);

    if (mainWorkflow?.variables && mainSync.config.variableMappings) {
      for (const mapping of mainSync.config.variableMappings) {
        const varExists = mainWorkflow.variables.some(v => v.name === mapping.externalName);
        if (!varExists) {
          errors.push(new ConfigurationValidationError(
            `Paired SYNC '${mainSync.nodeId}' (pairId: '${pairId}') maps external variable '${mapping.externalName}' which is not defined in workflow '${mainSync.graphId}'`,
            { configType: "workflow", context: { code: "PAIRED_SYNC_VARIABLE_NOT_FOUND", pairId, nodeId: mainSync.nodeId, variableName: mapping.externalName, workflowId: mainSync.graphId } }
          ));
        }
      }
    }

    if (subWorkflow?.variables && subSync.config.variableMappings) {
      for (const mapping of subSync.config.variableMappings) {
        const varExists = subWorkflow.variables.some(v => v.name === mapping.externalName);
        if (!varExists) {
          errors.push(new ConfigurationValidationError(
            `Paired SYNC '${subSync.nodeId}' (pairId: '${pairId}') maps external variable '${mapping.externalName}' which is not defined in workflow '${subSync.graphId}'`,
            { configType: "workflow", context: { code: "PAIRED_SYNC_VARIABLE_NOT_FOUND", pairId, nodeId: subSync.nodeId, variableName: mapping.externalName, workflowId: subSync.graphId } }
          ));
        }
      }
    }
  }
}

function validateForkContextCompatibility(
  mainPairMap: Map<ID, SyncNodeInfo>,
  _subPairMap: Map<ID, SyncNodeInfo>,
  _mainGraph: WorkflowGraphData,
  _errors: ConfigurationValidationError[],
): void {
  // Check if both paired SYNCs are within compatible FORK-JOIN scopes.
  // Implementation depends on whether the main/sub SYNCs declare sourcePathId
  // that reference valid FORK paths in their respective graphs.
  // This validation is less strict — FORK context is determined at runtime
  // by the execution engine, not statically by the graph structure.
}
```

### 5.5 集成到预处理流程

在 [GraphValidator](file:///d:/项目/agent/wf-agent/sdk/workflow/validation/graph-validation/graph-validator.ts) 中新增跨图验证钩子：

```typescript
export class GraphValidator {
  static validate(
    graph: WorkflowGraphData,
    options?: {
      subworkflowGraphs?: Map<ID, WorkflowGraphData>;
      workflowRegistry?: WorkflowRegistry;
    },
  ): Result<WorkflowGraphData, ConfigurationValidationError[]> {
    // ... 现有单图验证保持不变 ...

    // 新增：跨图 paired SYNC 验证
    if (options?.subworkflowGraphs && options?.workflowRegistry) {
      const pairedSyncErrors = validatePairedSyncCrossGraph(
        graph,
        options.subworkflowGraphs,
        options.workflowRegistry,
      );
      errorList.push(...pairedSyncErrors);
    }

    // ... 返回结果 ...
  }
}
```

在 [WorkflowProcessor](file:///d:/项目/agent/wf-agent/sdk/core/graph/workflow-processor.ts) 的 `preprocessWorkflow` 方法中传递参数：

```typescript
const graphValidationResult = GraphValidator.validate(buildResult.graph, {
  subworkflowGraphs: this.subworkflowGraphCache,  // 预处理过程中缓存的子图
  workflowRegistry: this.workflowRegistry,
});
```

## 6. 测试策略

### 6.1 图内验证测试用例（新增）

| 测试用例 | 验证规则 | 期望结果 |
|----------|----------|----------|
| 有效 pairId 格式验证通过 | R1 | 无错误 |
| 无效 pairId 格式报错 | R1 | `INVALID_PAIR_ID_FORMAT` |
| 同一图内重复 pairId 报错 | R1 | `DUPLICATE_PAIR_ID_IN_GRAPH` |
| paired SYNC 无 exchange 报错 | R2 | `PAIRED_SYNC_NO_EXCHANGE` |
| exchange sourcePathId 不存在报错 | R2 | `INVALID_EXCHANGE_SOURCE_PATH_ID` |
| exchange 自身引用报错 | R3 | `SELF_REFERENCE_EXCHANGE` |
| exchange 重复 source 报错 | R3 | `DUPLICATE_EXCHANGE_SOURCE` |
| paired SYNC 不与 SUBGRAPH 连接报错 | R5 | `PAIRED_SYNC_INVALID_POSITION` |
| paired SYNC 与 SUBGRAPH 正确连接通过 | R5 | 无错误 |

### 6.2 跨图验证测试用例（新增）

| 测试用例 | 验证规则 | 期望结果 |
|----------|----------|----------|
| pairId 在主/子图各出现一次 | R6 | 无错误 |
| pairId 在子图缺少配对 | R6 | `PAIR_ID_NOT_SYMMETRIC` |
| pairId 在主图缺少配对 | R6 | `PAIR_ID_NOT_SYMMETRIC` |
| 配对 SYNC 变量在工作流中存在 | R8 | 无错误 |
| 配对 SYNC 变量在工作流中不存在 | R8 | `PAIRED_SYNC_VARIABLE_NOT_FOUND` |

## 7. 实现建议

### 阶段 1：类型定义 + Zod Schema + 图内验证

1. 为 [SyncNodeConfig](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/sync-configs.ts) 新增 `pairId` 和 `variableExchanges` 字段
2. 为 [sync-configs-schema.ts](file:///d:/项目/agent/wf-agent/packages/types/src/node/configs/sync-configs-schema.ts) 新增 Zod Schema
3. 导出新增类型
4. 在 [sync-node-validator.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/validation/graph-validation/sync-node-validator.ts) 中新增图内验证函数集

### 阶段 2：跨图验证

1. 创建 [sync-pair-cross-graph-validator.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/validation/graph-validation/sync-pair-cross-graph-validator.ts)
2. 集成到 [GraphValidator](file:///d:/项目/agent/wf-agent/sdk/workflow/validation/graph-validation/graph-validator.ts) 的可选参数
3. 更新 [WorkflowProcessor](file:///d:/项目/agent/wf-agent/sdk/core/graph/workflow-processor.ts) 传递子图缓存
4. 从 [index.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/validation/graph-validation/index.ts) 导出新模块

### 阶段 3：测试覆盖

1. 在 [sync-node-validator.test.ts](file:///d:/项目/agent/wf-agent/sdk/workflow/validation/graph-validation/__tests__/sync-node-validator.test.ts) 中新增图内验证测试
2. 创建跨图验证测试文件

## 8. 与现有系统的兼容性

- 未设置 `pairId` 的 SYNC 节点保持现有行为，**零影响**
- `variableExchanges` 和 `variableMappings` 可同时存在，验证器各自独立验证
- 跨图验证通过 `GraphValidator` 的可选参数启用，不影响现有调用方
- 错误码体系保持 `ConfigurationValidationError` + `context.code` 模式

## 9. 总结

| 维度 | 当前能力 | 改进后能力 |
|------|----------|-----------|
| 验证范围 | 单图内部 | 单图 + 跨图 |
| SYNC 方向 | 单向（source→target） | 双向（paired exchange） |
| 配置字段 | sourcePathId/targetPathId/variableMappings | 新增 pairId/variableExchanges |
| 验证时机 | GraphValidator.validate() | validate() + cross-graph 后处理 |
| 错误码 | 7 个 | 14+ 个 |
| 测试覆盖 | 12 个测试 | 需要 30+ 个测试 |