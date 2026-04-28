# 命名重构设计方案

## 概述

本文档描述将 `graph-agent` 项目重命名为 `wf-agent`，并统一内部命名体系的完整设计方案。

## 当前命名问题分析

### 1. 概念混淆

当前项目中存在以下命名层次：

```
WorkflowDefinition (静态定义)
    ↓ 预处理/构建
PreprocessedGraph / GraphData (中间结构)
    ↓ 执行
Thread (执行实例)
```

**问题：**
- `graph` 模块名不能准确表达"工作流执行引擎"的职责
- `thread` 命名与 `agent` 不对称，两者都是执行实例概念
- `WorkflowDefinition` 与 `Thread` 的关系不够直观

### 2. 与业界术语不一致

- 业界普遍使用 **Workflow** 描述工作流系统
- LangChain、Temporal 等框架都使用 Workflow 作为核心概念
- `graph-agent` 项目名对初次接触者不够直观

## 目标命名体系

### 核心概念层次

```
WorkflowTemplate (静态定义层)
    ↓ 预处理/构建
WorkflowGraph (可执行图结构)
    ↓ 实例化执行
WorkflowExecution (执行实例层)
```

### 与 Agent 的对称关系

```
sdk/
├── agent/                    # 智能体执行模块
│   ├── AgentLoopTemplate     # 配置模板
│   ├── AgentLoop             # 执行实例
│   └── AgentLoopExecutor     # 执行器
│
└── workflow/                 # 工作流执行模块（原 graph）
    ├── WorkflowTemplate      # 配置模板
    ├── WorkflowGraph         # 预处理后的图
    ├── WorkflowExecution     # 执行实例
    └── WorkflowExecutor      # 执行器
```

## 详细命名映射

### 1. 项目级别

| 当前 | 目标 | 说明 |
|------|------|------|
| `graph-agent` | `f-agent` | 项目名称 |

### 2. SDK 模块级别

| 当前 | 目标 | 说明 |
|------|------|------|
| `sdk/graph/` | `sdk/workflow/` | 工作流执行模块 |

### 3. 类型定义（packages/types）

#### 3.1 工作流定义层

| 当前 | 目标 | 文件路径 |
|------|------|----------|
| `WorkflowDefinition` | `WorkflowTemplate` | `src/workflow/definition.ts` |
| `WorkflowType` | `WorkflowTemplateType` | `src/workflow/type.ts` |

#### 3.2 图结构层

| 当前 | 目标 | 文件路径 |
|------|------|----------|
| `PreprocessedGraph` | `WorkflowGraph` | `src/graph/preprocessed-graph.ts` |
| `GraphData` | `WorkflowGraphData` | 需新建文件 |
| `Graph` (interface) | `WorkflowGraphStructure` | `src/graph/structure.ts` |
| `GraphNode` | `WorkflowNode` | `src/graph/structure.ts` |
| `GraphEdge` | `WorkflowEdge` | `src/graph/structure.ts` |
| `GraphAnalysisResult` | `WorkflowGraphAnalysis` | `src/graph/analysis.ts` |

#### 3.3 执行实例层（原 Thread）

| 当前 | 目标 | 文件路径 |
|------|------|----------|
| `Thread` | `WorkflowExecution` | `src/thread/definition.ts` |
| `ThreadEntity` | `WorkflowExecutionEntity` | SDK 中 |
| `ThreadType` | `WorkflowExecutionType` | `src/thread/status.ts` |
| `ThreadStatus` | `WorkflowExecutionStatus` | `src/thread/status.ts` |
| `ThreadVariable` | `WorkflowExecutionVariable` | `src/thread/variables.ts` |
| `ThreadResult` | `WorkflowExecutionResult` | `src/result.ts` |

### 4. SDK 实现层

#### 4.1 目录结构重命名

```
sdk/graph/
├── graph-builder/          → workflow-builder/
│   ├── graph-builder.ts    → workflow-graph-builder.ts
│   ├── graph-navigator.ts  → workflow-navigator.ts
│   └── utils/
│       ├── graph-analyzer.ts      → workflow-graph-analyzer.ts
│       ├── graph-cycle-detector.ts → workflow-cycle-detector.ts
│       ├── graph-reachability-analyzer.ts → workflow-reachability-analyzer.ts
│       ├── graph-topological-sorter.ts → workflow-topological-sorter.ts
│       └── graph-traversal.ts      → workflow-traversal.ts
│
├── entities/
│   ├── graph-data.ts       → workflow-graph-data.ts
│   ├── preprocessed-graph-data.ts → workflow-graph.ts
│   └── thread-entity.ts    → workflow-execution-entity.ts
│
├── execution/
│   ├── executors/
│   │   └── thread-executor.ts → workflow-executor.ts
│   ├── thread-pool.ts      → workflow-execution-pool.ts
│   ├── thread-execution-context.ts → workflow-execution-context.ts
│   └── coordinators/
│       ├── thread-execution-coordinator.ts → workflow-execution-coordinator.ts
│       ├── thread-lifecycle-coordinator.ts → workflow-lifecycle-coordinator.ts
│       ├── thread-operation-coordinator.ts → workflow-operation-coordinator.ts
│       └── thread-state-transitor.ts → workflow-state-transitor.ts
│
├── state-managers/
│   ├── thread-state.ts     → workflow-execution-state.ts
│   └── thread-state-coordinator.ts → workflow-state-coordinator.ts
│
└── stores/
    └── thread-registry.ts  → workflow-execution-registry.ts
```

#### 4.2 类/接口重命名

| 当前 | 目标 |
|------|------|
| `GraphBuilder` | `WorkflowGraphBuilder` |
| `GraphNavigator` | `WorkflowNavigator` |
| `GraphValidator` | `WorkflowGraphValidator` |
| `ThreadBuilder` | `WorkflowExecutionBuilder` |
| `ThreadExecutor` | `WorkflowExecutor` |
| `ThreadPool` | `WorkflowExecutionPool` |
| `ThreadRegistry` | `WorkflowExecutionRegistry` |
| `ThreadExecutionContext` | `WorkflowExecutionContext` |

### 5. API 层

```
sdk/api/graph/              → sdk/api/workflow/
├── operations/
│   └── execution/
│       ├── execute-thread-command.ts        → execute-workflow-command.ts
│       ├── execute-thread-stream-command.ts → execute-workflow-stream-command.ts
│       ├── pause-thread-command.ts          → pause-workflow-command.ts
│       ├── resume-thread-command.ts         → resume-workflow-command.ts
│       └── cancel-thread-command.ts         → cancel-workflow-command.ts
│
└── resources/
    └── threads/
        └── thread-registry-api.ts           → workflow-execution-registry-api.ts
```

### 6. 事件系统

| 当前 | 目标 |
|------|------|
| `ThreadStartData` | `WorkflowExecutionStartData` |
| `ThreadEndData` | `WorkflowExecutionEndData` |
| `ThreadNodeData` | `WorkflowExecutionNodeData` |
| `ThreadForkData` | `WorkflowExecutionForkData` |
| `ThreadJoinData` | `WorkflowExecutionJoinData` |
| `ThreadMessageType` | `WorkflowExecutionMessageType` |

### 7. 存储层

| 当前 | 目标 |
|------|------|
| `ThreadStorage` | `WorkflowExecutionStorage` |
| `ThreadSnapshot` | `WorkflowExecutionSnapshot` |

### 8. 检查点相关

| 当前 | 目标 |
|------|------|
| `ThreadCheckpoint` | `WorkflowExecutionCheckpoint` |
| `ThreadSnapshotManager` | `WorkflowExecutionSnapshotManager` |

## 命名约定规则

### 1. 复合命名模式

所有工作流相关命名遵循以下模式：

```
Workflow + [Component] + [Role]

示例：
- WorkflowGraphBuilder    (图构建器)
- WorkflowNavigator       (导航器)
- WorkflowExecutionPool   (执行池)
- WorkflowStateCoordinator (状态协调器)
```

### 2. 缩写规范

为避免命名过长，允许以下缩写：

| 完整 | 缩写 | 使用场景 |
|------|------|----------|
| `WorkflowExecution` | `WfExecution` | 内部私有变量 |
| `WorkflowGraph` | `WfGraph` | 内部私有变量 |
| `WorkflowTemplate` | `WfTemplate` | 内部私有变量 |

**注意：** 公共 API 和类型定义使用完整名称，仅在内部实现中使用缩写。

### 3. 文件命名规范

- 使用 `kebab-case` 命名文件
- 文件前缀与模块名一致

```
workflow-executor.ts          ✓
workflow_execution_pool.ts    ✗
WorkflowExecutor.ts           ✗
```

## 与其他模块的关系

### 与 Agent 模块的对比

| 维度 | Agent 模块 | Workflow 模块 |
|------|------------|---------------|
| 配置模板 | `AgentLoopTemplate` | `WorkflowTemplate` |
| 执行实例 | `AgentLoop` | `WorkflowExecution` |
| 执行器 | `AgentLoopExecutor` | `WorkflowExecutor` |
| 状态管理 | `AgentLoopState` | `WorkflowExecutionState` |
| 注册表 | `AgentLoopRegistry` | `WorkflowExecutionRegistry` |

### 与 Core 模块的关系

`core/` 模块包含共享组件，命名保持不变：

```
core/
├── execution/          # 执行池（共享）
├── llm/               # LLM 客户端
├── tools/             # 工具执行
└── triggers/          # 触发器系统
```

## 向后兼容性策略

### 1. 类型别名

在重构完成后，提供类型别名以支持平滑迁移：

```typescript
// packages/types/src/compat/index.ts
/** @deprecated Use WorkflowExecution instead */
export type Thread = WorkflowExecution;

/** @deprecated Use WorkflowGraph instead */
export type PreprocessedGraph = WorkflowGraph;
```

### 2. 逐步弃用

- Phase 1-3：内部使用新命名，保持公共 API 不变
- Phase 4：更新公共 API，提供类型别名
- Phase 5：完全移除旧命名（可选）

## 预期收益

### 1. 概念清晰

- 明确的层次结构：Template → Graph → Execution
- 与 Agent 模块形成对称设计
- 符合领域驱动设计原则

### 2. 易于理解

- `workflow-agent` 项目名称直观
- 新开发者更容易理解架构
- 与业界标准术语一致

### 3. 可维护性

- 命名一致性提高代码可读性
- 减少概念混淆导致的错误
- 便于文档编写和知识传递

## 风险评估

### 高风险项

1. **大量文件重命名**：可能影响 Git 历史追溯
2. **类型定义变更**：需要同步更新所有依赖
3. **API 变更**：影响外部使用者

### 缓解措施

1. 分阶段执行，每阶段充分测试
2. 保持类型别名支持向后兼容
3. 提供详细的迁移文档
4. 在主要版本更新中发布（如 v2.0）

## 总结

本次命名重构旨在建立清晰、一致、符合业界标准的命名体系。通过将 `graph-agent` 重命名为 `wf-agent`，并统一内部命名，可以显著提高项目的可理解性和可维护性。

关键变更：
- `graph-agent` → `wf-agent`
- `sdk/graph/` → `sdk/workflow/`
- `Thread` → `WorkflowExecution`
- `GraphData` → `WorkflowGraphData`
- `PreprocessedGraph` → `WorkflowGraph`
- `WorkflowDefinition` → `WorkflowTemplate`
