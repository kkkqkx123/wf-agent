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
| `MANUAL` | On-demand | Via API or command |
| `ON_ERROR` | On node failure | Global policy |

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