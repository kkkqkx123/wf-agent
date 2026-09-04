# Checkpoint Mechanism

## 1. Overview

The checkpoint mechanism provides state snapshot and restoration for workflow executions. It enables:

- **Pause/Resume**: Suspend execution and resume later
- **Fault Recovery**: Recover from failures by restoring to the last checkpoint
- **Incremental Checkpoints**: Only store delta changes between checkpoints
- **Hierarchical Checkpoints**: Support for subgraph and fork branch checkpoints

## 2. Architecture

```
CheckpointCoordinator (extends BaseCheckpointCoordinator)
├── createWorkflowCheckpoint()
│   ├── Build checkpoint from entity state
│   ├── Extract state from WorkflowExecutionEntity
│   ├── Apply snapshot size budget
│   ├── Handle incremental message storage
│   └── Persist to storage
│
├── restoreWorkflowFromCheckpoint()
│   ├── Load checkpoint from storage
│   ├── Build entity from snapshot
│   ├── Restore state (execution, fork/join, variables)
│   ├── Restore child executions (agent loops, sub-workflows)
│   └── Post-restore hooks
│
└── createCheckpointWithStrategy()
    ├── CheckpointStrategy determines when to create checkpoints
    ├── Supports NODE_BEFORE, NODE_AFTER, MANUAL triggers
    └── Configurable per-node and global
```

## 3. Checkpoint Types

| Trigger Type | Timing | Configuration |
|-------------|--------|---------------|
| `BEFORE_EXECUTE` | Before each node execution | Per-node or global |
| `AFTER_EXECUTE` | After each node execution | Per-node or global |
| `ON_ERROR` | On node failure | Global policy |
| `MANUAL` | Workflow start | Workflow scope（不受节点配置影响） |
| `ON_COMPLETE` | Workflow end（在 `complete()` 之后） | Workflow scope |
| `ON_PAUSE` | 观测到 Paused 中断时 | 始终落盘（不经策略门控） |
| `ON_CANCEL` | 观测到 Stopped 中断时 | 始终落盘（不经策略门控） |
| `ON_TIMEOUT` | 超出 `max_execution_time` 时 | 始终落盘（不经策略门控） |

> `ON_PAUSE` / `ON_CANCEL` / `ON_TIMEOUT` 三个中断触发与节点级策略无关：
> 无论 `NodeCheckpointStrategy` 如何配置都会落盘，保证被暂停/取消/超时的执行
> 都能从存储恢复或审计。三者在 `check_interruption_and_timeout` 中分别对应
> `Paused` / `Stopped` / 超时分支。

## 4. State Extraction

The `extractState()` method collects all serializable state from the entity:

```
Checkpoint State:
├── Execution state: status, startTime, endTime, currentNodeId, errorChain
├── Node results: completed node execution results
├── Variables: all scoped variables
├── Fork/Join state: fork path IDs, aggregation state
├── Subgraph stack: subgraph execution context
├── Messages: conversation session messages (incremental)
├── Hierarchy metadata: parent-child relationships
├── Trigger state: trigger runtime state
└── Interruption state: pause/stop signals
```

### 实际落盘字段

`WorkflowCheckpointIntegration::build_snapshot` 当前填充的字段（其余为 `None`）：

| 字段 | 状态 |
|------|------|
| `executionId` / `status` / `currentNodeId` | 已捕获 |
| `variableState` / `nodeResults` / `nodeExecutionRecords` | 已捕获 |
| `errorRecords` / `interruptionRecords` / `eventRecords` | 已捕获 |
| `triggerStates` | 已捕获（需 `with_trigger_state_registry` 注入） |
| `hierarchy` / `executionConfig` / `forkJoinContext` | **未捕获**（始终为 `None`） |
| `forkJoinAggregationState` / `hookExecutionContext` | **未捕获**（始终为 `None`） |
| `input` / `output` / `messages` / `conversationState` | **未捕获**（始终为 `None`） |

> `status` 字段是 `ExecutionStatus` 的 Debug 字符串。中断触发（`ON_PAUSE` / `ON_CANCEL` /
> `ON_TIMEOUT`）在实体状态落定**之后**写快照，因此 `Paused` / `Cancelled` / `Timeout`
> 都能被真实记录；`Timeout` 不再坍缩成 `Failed`。

## 5. Checkpoint Strategies

Configurable via `NodeCheckpointStrategy`:

- **Global config**: Applied to all nodes by default
- **Per-node config**: Overrides global config for specific nodes
- **Layer resolution**: Global → workflow → node → trigger-specific

## 6. Child Checkpoint Restoration

The `CheckpointCoordinator` handles hierarchical restoration:

```
restoreWorkflowFromCheckpoint():
  1. Load checkpoint metadata
  2. Build WorkflowExecutionEntity from snapshot
  3. Restore execution state
  4. Restore child agent loops (via AgentLoopCheckpointCoordinator)
  5. Restore child workflow executions recursively
  6. Run post-restore hooks
  7. Resume execution from the saved currentNodeId
```

## 7. Checkpoint Policies

- `WorkflowCheckpointPolicy` — Defines global checkpoint configuration
- `AgentLoopCheckpointPolicy` — Agent-specific checkpoint policy
- Supports file-based and in-memory storage backends