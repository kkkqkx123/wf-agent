# 命名重构分阶段实施方案

本文档详细描述命名重构的分阶段实施计划，确保重构过程可控、风险最小化。

## 总体策略

- **渐进式重构**：分 5 个阶段逐步完成
- **向后兼容**：每个阶段保持现有功能正常
- **充分测试**：每个阶段完成后进行全面测试
- **类型别名**：提供旧类型别名支持平滑过渡

---

## Phase 1: 类型定义层重构（预计 2-3 天）

### 目标

完成 `packages/types` 中的核心类型重命名。

### 任务清单

#### 1.1 工作流定义层

- [ ] 重命名 `WorkflowDefinition` → `WorkflowTemplate`
  - 文件：`src/workflow/definition.ts`
  - 更新所有引用
  
- [ ] 重命名 `WorkflowType` → `WorkflowTemplateType`
  - 文件：`src/workflow/type.ts`

#### 1.2 图结构层

- [ ] 重命名 `PreprocessedGraph` → `WorkflowGraph`
  - 文件：`src/graph/preprocessed-graph.ts`
  
- [ ] 重命名 `Graph` (interface) → `WorkflowGraphStructure`
  - 文件：`src/graph/structure.ts`
  
- [ ] 重命名 `GraphNode` → `WorkflowNode`
  - 文件：`src/graph/structure.ts`
  
- [ ] 重命名 `GraphEdge` → `WorkflowEdge`
  - 文件：`src/graph/structure.ts`

#### 1.3 Thread 执行层

- [ ] 重命名 `Thread` → `WorkflowExecution`
  - 文件：`src/thread/definition.ts`
  
- [ ] 重命名 `ThreadType` → `WorkflowExecutionType`
  - 文件：`src/thread/status.ts`
  
- [ ] 重命名 `ThreadStatus` → `WorkflowExecutionStatus`
  - 文件：`src/thread/status.ts`

#### 1.4 目录结构调整

```
packages/types/src/
├── workflow/
│   └── definition.ts       # WorkflowTemplate (原 WorkflowDefinition)
├── graph/
│   ├── structure.ts        # WorkflowGraphStructure, WorkflowNode, WorkflowEdge
│   └── preprocessed-graph.ts # WorkflowGraph (原 PreprocessedGraph)
└── thread/
    └── definition.ts       # WorkflowExecution (原 Thread)
```

### 验证步骤

1. 运行类型检查：`cd packages/types && pnpm build`
2. 确保无类型错误
3. 运行类型包测试：`pnpm test`

### 回滚策略

- 使用 Git 分支：`feature/phase1-type-renaming`
- 如出现问题，直接丢弃分支重新开发

---

## Phase 2: SDK 实体层重构（预计 3-4 天）

### 目标

完成 SDK 中实体类的重命名，基于 Phase 1 的新类型。

### 前置条件

- Phase 1 已完成并通过测试
- `packages/types` 已发布或本地链接更新

### 任务清单

#### 2.1 实体类重命名

- [ ] 重命名 `GraphData` → `WorkflowGraphData`
  - 文件：`sdk/graph/entities/graph-data.ts`
  - 新路径：`sdk/workflow/entities/workflow-graph-data.ts`
  
- [ ] 重命名 `PreprocessedGraphData` → `WorkflowGraph`
  - 文件：`sdk/graph/entities/preprocessed-graph-data.ts`
  - 新路径：`sdk/workflow/entities/workflow-graph.ts`
  
- [ ] 重命名 `ThreadEntity` → `WorkflowExecutionEntity`
  - 文件：`sdk/graph/entities/thread-entity.ts`
  - 新路径：`sdk/workflow/entities/workflow-execution-entity.ts`

#### 2.2 状态管理类重命名

- [ ] 重命名 `ThreadState` → `WorkflowExecutionState`
  - 文件：`sdk/graph/state-managers/thread-state.ts`
  
- [ ] 重命名 `ThreadStateCoordinator` → `WorkflowStateCoordinator`
  - 文件：`sdk/graph/state-managers/thread-state-coordinator.ts`

#### 2.3 注册表重命名

- [ ] 重命名 `ThreadRegistry` → `WorkflowExecutionRegistry`
  - 文件：`sdk/graph/stores/thread-registry.ts`
  
- [ ] 重命名 `GraphRegistry` → `WorkflowGraphRegistry`
  - 文件：`sdk/graph/stores/graph-registry.ts`

### 验证步骤

1. 运行 SDK 构建：`cd sdk && pnpm build`
2. 运行实体相关测试：`pnpm test entities`
3. 确保所有导入路径正确

### 依赖关系

```
Phase 2 依赖 Phase 1 完成
│
└── 使用新的类型定义
    ├── WorkflowExecution (原 Thread)
    ├── WorkflowGraph (原 PreprocessedGraph)
    └── WorkflowGraphData (原 GraphData)
```

---

## Phase 3: SDK 执行层重构（预计 4-5 天）

### 目标

完成执行相关类的重命名，这是改动最大的阶段。

### 任务清单

#### 3.1 构建器重命名

- [ ] 重命名 `GraphBuilder` → `WorkflowGraphBuilder`
  - 文件：`sdk/graph/graph-builder/graph-builder.ts`
  - 新路径：`sdk/workflow/builder/workflow-graph-builder.ts`
  
- [ ] 重命名 `GraphNavigator` → `WorkflowNavigator`
  - 文件：`sdk/graph/graph-builder/graph-navigator.ts`
  - 新路径：`sdk/workflow/builder/workflow-navigator.ts`

#### 3.2 图工具类重命名

- [ ] 重命名 `GraphAnalyzer` → `WorkflowGraphAnalyzer`
- [ ] 重命名 `GraphCycleDetector` → `WorkflowCycleDetector`
- [ ] 重命名 `GraphReachabilityAnalyzer` → `WorkflowReachabilityAnalyzer`
- [ ] 重命名 `GraphTopologicalSorter` → `WorkflowTopologicalSorter`
- [ ] 重命名 `GraphTraversal` → `WorkflowTraversal`

#### 3.3 执行器重命名

- [ ] 重命名 `ThreadExecutor` → `WorkflowExecutor`
  - 文件：`sdk/graph/execution/executors/thread-executor.ts`
  - 新路径：`sdk/workflow/execution/workflow-executor.ts`
  
- [ ] 重命名 `ThreadPool` → `WorkflowExecutionPool`
  - 文件：`sdk/graph/execution/thread-pool.ts`
  - 新路径：`sdk/workflow/execution/workflow-execution-pool.ts`

#### 3.4 协调器重命名

- [ ] 重命名 `ThreadExecutionCoordinator` → `WorkflowExecutionCoordinator`
- [ ] 重命名 `ThreadLifecycleCoordinator` → `WorkflowLifecycleCoordinator`
- [ ] 重命名 `ThreadOperationCoordinator` → `WorkflowOperationCoordinator`
- [ ] 重命名 `ThreadStateTransitor` → `WorkflowStateTransitor`

#### 3.5 上下文重命名

- [ ] 重命名 `ThreadExecutionContext` → `WorkflowExecutionContext`
  - 文件：`sdk/graph/execution/thread-execution-context.ts`

### 目录结构调整

```
sdk/workflow/                    # 原 sdk/graph/
├── builder/                     # 原 graph-builder/
│   ├── workflow-graph-builder.ts
│   ├── workflow-navigator.ts
│   └── utils/
│       ├── workflow-graph-analyzer.ts
│       ├── workflow-cycle-detector.ts
│       ├── workflow-reachability-analyzer.ts
│       ├── workflow-topological-sorter.ts
│       └── workflow-traversal.ts
│
├── entities/
│   ├── workflow-graph-data.ts
│   ├── workflow-graph.ts
│   └── workflow-execution-entity.ts
│
├── execution/
│   ├── executors/
│   │   └── workflow-executor.ts
│   ├── workflow-execution-context.ts
│   ├── workflow-execution-pool.ts
│   └── coordinators/
│       ├── workflow-execution-coordinator.ts
│       ├── workflow-lifecycle-coordinator.ts
│       ├── workflow-operation-coordinator.ts
│       └── workflow-state-transitor.ts
│
├── state-managers/
│   ├── workflow-execution-state.ts
│   └── workflow-state-coordinator.ts
│
└── stores/
    ├── workflow-graph-registry.ts
    └── workflow-execution-registry.ts
```

### 验证步骤

1. 运行完整构建：`pnpm build`
2. 运行 SDK 测试：`cd sdk && pnpm test`
3. 重点测试执行流程：`pnpm test execution`
4. 检查所有导入和依赖

---

## Phase 4: API 层和事件系统重构（预计 2-3 天）

### 目标

完成 API 层和事件系统的重命名，提供向后兼容的类型别名。

### 任务清单

#### 4.1 API 层重命名

- [ ] 重命名目录 `sdk/api/graph/` → `sdk/api/workflow/`

- [ ] 重命名命令文件
  - `execute-thread-command.ts` → `execute-workflow-command.ts`
  - `execute-thread-stream-command.ts` → `execute-workflow-stream-command.ts`
  - `pause-thread-command.ts` → `pause-workflow-command.ts`
  - `resume-thread-command.ts` → `resume-workflow-command.ts`
  - `cancel-thread-command.ts` → `cancel-workflow-command.ts`

- [ ] 重命名 API 类
  - `ThreadRegistryAPI` → `WorkflowExecutionRegistryAPI`

#### 4.2 事件系统重命名

- [ ] 重命名事件数据类型
  - `ThreadStartData` → `WorkflowExecutionStartData`
  - `ThreadEndData` → `WorkflowExecutionEndData`
  - `ThreadNodeData` → `WorkflowExecutionNodeData`
  - `ThreadForkData` → `WorkflowExecutionForkData`
  - `ThreadJoinData` → `WorkflowExecutionJoinData`

- [ ] 重命名事件类型枚举
  - `ThreadMessageType` → `WorkflowExecutionMessageType`

#### 4.3 添加向后兼容类型别名

在 `packages/types/src/compat/index.ts` 中添加：

```typescript
/**
 * @deprecated Use WorkflowExecution instead. Will be removed in v3.0.
 */
export type Thread = WorkflowExecution;

/**
 * @deprecated Use WorkflowGraph instead. Will be removed in v3.0.
 */
export type PreprocessedGraph = WorkflowGraph;

/**
 * @deprecated Use WorkflowTemplate instead. Will be removed in v3.0.
 */
export type WorkflowDefinition = WorkflowTemplate;

// ... 其他别名
```

### 验证步骤

1. 运行 API 测试：`pnpm test api`
2. 运行事件系统测试：`pnpm test events`
3. 检查类型别名是否正确导出

---

## Phase 5: 项目级重命名和清理（预计 1-2 天）

### 目标

完成项目级重命名和最终清理。

### 任务清单

#### 5.1 项目配置更新

- [ ] 更新 `package.json` 中的项目名称
  ```json
  {
    "name": "workflow-agent"
  }
  ```

- [ ] 更新 `pnpm-workspace.yaml` 中的包名引用

- [ ] 更新所有 `package.json` 中的包名引用

#### 5.2 文档更新

- [ ] 更新 `README.md`
- [ ] 更新 `AGENTS.md`
- [ ] 更新 `sdk/README.md`
- [ ] 更新所有架构文档中的命名引用

#### 5.3 配置文件更新

- [ ] 更新 `tsconfig.json` 中的路径映射（如有）
- [ ] 更新构建脚本中的引用
- [ ] 更新测试配置

#### 5.4 最终清理（可选，v3.0 时执行）

- [ ] 移除 `packages/types/src/compat/` 目录
- [ ] 移除所有 `@deprecated` 类型别名
- [ ] 更新文档移除兼容性说明

### 验证步骤

1. 完整构建项目：`pnpm build`
2. 运行所有测试：`pnpm test`
3. 检查 CLI 应用：`cd apps/cli-app && pnpm build`
4. 验证包名：`pnpm list`

---

## 时间线规划

```
Week 1:
├── Day 1-2: Phase 1 - 类型定义层
│   └── 完成 packages/types 重构
│
├── Day 3-4: Phase 2 - SDK 实体层
│   └── 完成实体类重命名
│
└── Day 5: Phase 1-2 测试和修复

Week 2:
├── Day 1-3: Phase 3 - SDK 执行层
│   └── 完成执行器、协调器重命名
│
├── Day 4: Phase 3 测试和修复
│
└── Day 5: Phase 4 - API 和事件系统

Week 3:
├── Day 1: Phase 4 完成和测试
│
├── Day 2: Phase 5 - 项目级重命名
│
├── Day 3-4: 全面测试和文档更新
│
└── Day 5: 发布准备
```

---

## 测试策略

### 每阶段测试清单

- [ ] 单元测试通过
- [ ] 集成测试通过
- [ ] 类型检查通过
- [ ] 构建成功
- [ ] CLI 应用正常运行

### 关键测试场景

1. **工作流执行测试**
   - 简单工作流执行
   - Fork/Join 并行执行
   - 子工作流调用
   - 检查点恢复

2. **Agent Loop 测试**
   - 确保 Agent 模块不受 Workflow 模块重构影响

3. **API 兼容性测试**
   - 旧类型别名是否正常工作
   - 新 API 是否正确导出

---

## 风险缓解

### 风险 1: 大规模文件变更导致合并冲突

**缓解措施：**
- 快速完成每个 Phase
- 冻结其他大功能开发
- 使用独立分支，完成后立即合并

### 风险 2: 测试覆盖率不足

**缓解措施：**
- 每阶段增加针对性测试
- 使用变异测试确保覆盖
- 手动验证关键路径

### 风险 3: 外部依赖破坏

**缓解措施：**
- 保持类型别名直到 v3.0
- 提供详细的迁移指南
- 在 CHANGELOG 中明确标注破坏性变更

---

## 回滚计划

如果在任何阶段出现严重问题：

1. **Phase 1-2**：丢弃分支，重新开始
2. **Phase 3**：回滚到 Phase 2 完成状态
3. **Phase 4-5**：修复问题或回滚到上一阶段

每个 Phase 完成后打标签：
```bash
git tag phase-1-complete
git tag phase-2-complete
...
```

---

## 发布计划

### v2.0.0（主要版本）

- 包含所有 Phase 1-5 的变更
- 提供向后兼容的类型别名
- 发布详细迁移指南

### v3.0.0（未来版本）

- 移除所有 `@deprecated` 类型别名
- 完全移除旧命名支持

---

## 总结

本实施计划通过 5 个阶段逐步完成命名重构：

1. **Phase 1**: 类型定义层（基础）
2. **Phase 2**: SDK 实体层（数据）
3. **Phase 3**: SDK 执行层（核心逻辑）
4. **Phase 4**: API 和事件系统（接口）
5. **Phase 5**: 项目级重命名（收尾）

每个阶段都有明确的任务清单、验证步骤和回滚策略，确保重构过程可控、风险最小化。

预计总工期：**2-3 周**（包含测试和修复时间）
