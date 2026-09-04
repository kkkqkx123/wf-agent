# Agent Checkpoint Mechanism

## 1. Overview

The agent checkpoint mechanism provides state snapshot and restoration for agent loop executions. It enables:

- **Pause/Resume**: Suspend execution and resume later
- **Fault Recovery**: Recover from failures by restoring to the last checkpoint
- **Incremental Checkpoints**: Only store delta changes between checkpoints
- **Hierarchical Checkpoints**: Support for child execution checkpoint restoration

## 2. Architecture

```
AgentLoopCheckpointCoordinator (extends BaseCheckpointCoordinator)
├── createCheckpoint()
│   ├── Build checkpoint from entity state
│   ├── Extract state from AgentLoopEntity
│   ├── Apply checkpoint policy (trigger, content config)
│   ├── Handle incremental message storage
│   └── Persist to storage
│
├── restoreFromCheckpoint()
│   ├── Load checkpoint from storage
│   ├── Build entity from snapshot
│   ├── Restore state (AgentLoopState)
│   ├── Restore child executions (agent loops, workflows)
│   └── Post-restore hooks
│
├── extractState()
│   ├── AgentLoopState (serialized)
│   ├── Message history (from ConversationSession)
│   ├── Execution hierarchy metadata
│   └── Trigger runtime state
│
└── buildEntityFromCheckpoint()
    ├── Create AgentLoopEntity from restored state
    ├── Recreate runtime managers
    └── Register with registries
```

### Checkpoint Dependencies

```typescript
interface CheckpointDependencies extends BaseCheckpointDependencies<AgentLoopCheckpoint> {
  saveCheckpoint: (checkpoint: AgentLoopCheckpoint) => Promise<string>;
  getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>;
  listCheckpoints: (agentLoopId: string) => Promise<string[]>;
  deltaConfig?: DeltaStorageConfig;
  conversationManager?: ConversationSession;
  fileCheckpointManager?: FileCheckpointManager;
  hierarchyRegistry?: ExecutionHierarchyRegistry;
  childCheckpointResolver?: ChildCheckpointResolver;
  workflowCoordinator?: CheckpointCoordinator;
}
```

## 3. Agent Checkpoint Policy

### Trigger Events

The policy defines which events trigger checkpoint creation:

| Trigger | Timing | Use Case | 接线状态 |
|---------|--------|----------|----------|
| `AFTER_EXECUTE` | After each iteration | Frequent checkpointing | 已接线（`AgentCheckpointTiming::AfterIteration`） |
| `ON_COMPLETE` | On loop completion | Final state capture（记录 `Completed`） | 已接线（`OnAgentEnd`） |
| `ON_ERROR` | On iteration error / terminal failure | Fault recovery | 已接线（`OnIterationError`） |
| `ON_PAUSE` | On pause | Resume support（记录 `Paused`） | 已接线（`OnAgentPause`） |
| `ON_CANCEL` | On cancel / stop | Cancel capture（记录 `Cancelled`/`Stopped`） | 已接线（`OnAgentCancel`） |
| `ON_TIMEOUT` | On wall-clock timeout | Timeout capture（记录 `Timeout`） | 已接线（`OnAgentTimeout`） |
| `MANUAL` | On agent start | Start capture | 已接线（`OnAgentStart`） |
| `ON_TOOL_CALL` / `ON_TOOL_RESULT` | Tool 前后 | Tool-level checkpoint | 工具通过 `ToolMetadata::create_checkpoint` opt-in |
| `BEFORE_EXECUTE` | Before each iteration | — | 已接线（`BeforeIteration`） |
| `ON_INTERVAL` | Periodic | Time-based checkpointing | 未接线 |
| `NEVER` | Never | Disable auto-checkpoint | 支持 |

> 说明：`OnAgentPause` 只在实体状态确实处于 `Paused` 时落盘（每次暂停一轮），避免重复快照；
> `OnAgentEnd`/`OnAgentCancel`/`OnAgentTimeout` 在终态**落定之后**由外层生命周期写入，
> 因此快照里的 `status` 字段是真实终态，而不是落定前的 `Running`。

### Default Policy

```typescript
const DEFAULT_AGENT_CHECKPOINT_POLICY: AgentCheckpointPolicy = {
  enabled: true,
  // `AgentCheckpointStrategy::every_iteration()`
  trigger: [AFTER_EXECUTE],
  content: { includeState: true, includeHistory: true, includeStatistics: false },
  retention: undefined, // 未配置时不过期清理
};
```

### 终态检查点策略

外层生命周期在执行结束后按**落定后的状态**选择触发类型并落盘一次：

| 落定状态 | 触发类型 |
|----------|----------|
| `Completed` | `ON_COMPLETE` |
| `Timeout` | `ON_TIMEOUT` |
| `Cancelled` / `Stopped` | `ON_CANCEL` |
| `Failed`（其他） | `ON_ERROR` |

### Content Configuration

Controls what data is included in the checkpoint:

```
AgentCheckpointContentConfig
├── includeState: boolean (include AgentLoopState)
├── includeMessages: boolean (include message history)
├── includeIterationHistory: boolean
├── includeToolCallRecords: boolean
└── includeTokenUsage: boolean
```

## 4. Checkpoint State

### What Gets Serialized

```
AgentLoopState:
├── _status: AgentLoopStatus
├── _currentIteration: number
├── _toolCallCount: number
├── _iterationHistory: IterationRecord[]
├── _startTime, _endTime: number
├── _error: ExecutionErrorRecord?
├── _errorChainManager state
└── _executionRecordManager state
```

### What Gets Serialized（实际落盘字段）

```
AgentCheckpoint（AgentStateSnapshot）
├── status / agentLoopId              执行状态；终态在落定之后写入
├── currentIteration / toolCallCount
├── conversationSnapshot              会话消息
├── variableSnapshots
├── error / startedAt / completedAt
├── iterationHistory / currentIterationRecord
├── pendingToolCallIds                崩溃时判断哪些工具调用仍在飞行中
├── toolDiscoveryState
├── isStreaming / streamMessage       流式中的部分消息
├── errorRecords / interruptionRecords / eventRecords   恢复时回填
└── toolCallHistory / triggerState / hierarchy / messages   未捕获（始终 None）
```

### What Does NOT Get Serialized

- **AgentLoopRuntimeConfig**: Contains unserializable functions (callbacks)
- **ConversationSession**: Messages are stored separately via delta/incremental storage
- **Runtime managers**: TimeoutManager, InterruptionState (recreated on restore)
- **未捕获字段**: `toolCallHistory` / `triggerState` / `hierarchy` / `messages` 目前仍写入 `None`

> `errorRecords` / `interruptionRecords` / `eventRecords` 必须捕获：
> `runtime_state_from_snapshot` 在恢复时会读取它们，若快照为空则这部分现场会静默丢失。
> `hierarchy` 与 `triggerState` 属于已知遗留缺口（workflow 侧的 `hierarchy` 同样未捕获）。

### Incremental Message Storage

Messages are stored incrementally to avoid duplicating the full conversation in each checkpoint:
- First checkpoint: full message history snapshot
- Subsequent checkpoints: only new messages (delta)
- On restore: full history is reconstructed from base + deltas

## 5. Checkpoint Restoration

### Restoration Flow

```
restoreFromCheckpoint(checkpointId, config):
  1. Load checkpoint data from storage
  2. Extract AgentLoopState snapshot
  3. Restore AgentLoopState (iteration history, status, etc.)
  4. Restore message history (base + deltas)
  5. Restore child executions:
     - Find child agent loops via hierarchy registry
     - Restore each child from its latest checkpoint
     - Rebuild parent-child relationships
  6. Create AgentLoopEntity with restored state
  7. Register entity with AgentLoopRegistry
  8. Return restored entity
```

### AgentLoopStateManager

Manages the lifecycle of checkpoints including creation, retrieval, deletion, and cleanup:

```
AgentLoopCheckpointStateManager (extends BaseCheckpointStateManager)
├── saveCheckpoint(checkpoint) → id
├── getCheckpoint(id) → checkpoint
├── listCheckpoints(options) → checkpoint IDs
├── deleteCheckpoint(id) → void
├── cleanup(entityId) → cleanup result
└── executeCleanupForEntity(entityId, type, excludeId) → void
```

## 6. Checkpoint Config Resolution

The checkpoint configuration is resolved from multiple layers:

```
CheckpointConfigLayer:
├── 1. Global defaults (lowest priority)
├── 2. Agent-specific config (from runtime config)
└── 3. Per-execution options (highest priority)
```

The `resolveAgentCheckpointConfig()` function merges these layers, and `buildAgentCheckpointLayers()` constructs the layered configuration for the checkpoint coordinator.

## 7. Checkpoint Strategies

The checkpoint strategy (`CheckpointStrategy`) determines when to create checkpoints:

- **Node-based**: BEFORE_EXECUTE, AFTER_EXECUTE (per iteration or event)
- **Manual**: Only on explicit request
- **Policy-driven**: Based on AgentCheckpointPolicy configuration

## 8. Child Execution Restoration

For agent loops that have child executions (sub-workflows, nested agent loops):

```
ChildCheckpointRestorer:
├── Find child executions via ExecutionHierarchyRegistry
├── Resolve latest checkpoint for each child
├── Restore each child entity
├── Rebuild parent-child relationships
└── Register restored children
```