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

| Trigger | Timing | Use Case |
|---------|--------|----------|
| `ON_ITERATION` | After each iteration | Frequent checkpointing |
| `ON_COMPLETE` | On loop completion | Final state capture |
| `ON_ERROR` | On error | Fault recovery |
| `ON_PAUSE` | On pause | Resume support |
| `ON_TOOL_CALL` | On tool call | Tool-level checkpoint |
| `ON_TOOL_RESULT` | On tool result | Post-tool state |
| `ON_INTERVAL` | Periodic | Time-based checkpointing |
| `MANUAL` | On-demand | Via API |
| `NEVER` | Never | Disable auto-checkpoint |

### Default Policy

```typescript
const DEFAULT_AGENT_CHECKPOINT_POLICY: AgentCheckpointPolicy = {
  enabled: true,
  trigger: [ON_ERROR, ON_PAUSE, ON_COMPLETE],
  content: { includeState: true, includeMessages: true },
  retention: { maxCheckpoints: 1000, maxAge: 7 days },
};
```

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

### What Does NOT Get Serialized

- **AgentLoopRuntimeConfig**: Contains unserializable functions (callbacks)
- **ConversationSession**: Messages are stored separately via delta/incremental storage
- **Runtime managers**: TimeoutManager, InterruptionState (recreated on restore)
- **Transient state**: Partial streaming messages, pending tool calls

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