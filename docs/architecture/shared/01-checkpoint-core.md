# Shared Checkpoint Core Infrastructure

## 1. Overview

The shared checkpoint infrastructure provides the base classes and utilities for both workflow and agent checkpoint mechanisms. It eliminates code duplication by providing a common foundation.

## 2. BaseCheckpointCoordinator

The base coordinator that both `WorkflowCheckpointCoordinator` and `AgentLoopCheckpointCoordinator` extend:

```
BaseCheckpointCoordinator<TCheckpoint>
├── createCheckpoint(entity, dependencies, options?) → Promise<string>
│   ├── Build checkpoint from entity state
│   ├── Handle checkpoint strategy (before/after/manual)
│   ├── Apply snapshot size budget
│   ├── Handle incremental message storage
│   └── Persist to storage
│
├── restoreFromCheckpoint(checkpointId, dependencies) → Promise<TEntity>
│   ├── Load checkpoint from storage
│   ├── Build entity from snapshot
│   ├── Restore state from snapshot
│   ├── Restore child executions
│   └── Post-restore hooks
│
├── extractState(entity) → CheckpointState
│   └── Collect all serializable state from entity
│
└── buildEntityFromCheckpoint(snapshot, dependencies) → TEntity
    └── Reconstruct entity from checkpoint data
```

### Design

- **Generic**: Parameterized by `TCheckpoint` type for workflow/agent-specific checkpoint types
- **Template Method**: Core algorithm defined in base, specific steps delegated to subclasses
- **Strategy Integration**: Works with `CheckpointStrategy` for checkpoint timing

## 3. BaseCheckpointStateManager

Manages the lifecycle of checkpoints in storage:

```
BaseCheckpointStateManager<TCheckpoint>
├── create(checkpoint) → Promise<string>
│   ├── Assign checkpoint ID
│   ├── Persist to storage adapter
│   ├── Emit CHECKPOINT_CREATED event
│   └── Return checkpoint ID
│
├── get(id) → Promise<TCheckpoint | null>
│   └── Load from storage adapter
│
├── list(filter?) → Promise<string[]>
│   └── Query checkpoint IDs with optional filtering
│
├── delete(id) → Promise<void>
│   ├── Remove from storage
│   └── Emit CHECKPOINT_DELETED event
│
├── cleanup(policy) → Promise<CleanupResult>
│   ├── Apply retention policy
│   ├── Remove expired checkpoints
│   └── Return cleanup stats
│
├── executeCleanupForEntity(entityId, entityType, excludeId) → Promise<void>
│   └── Entity-specific cleanup
│
└── setCleanupPolicy(policy) → void
    └── Configure cleanup policy
```

## 4. BaseDeltaRestorer

Handles incremental/delta checkpoint restoration:

```
BaseDeltaRestorer
├── restoreFromDelta(baseCheckpoint, deltaChain) → Checkpoint
│   ├── Apply each delta in sequence
│   ├── Handle version migration
│   └── Return reconstructed checkpoint
│
├── buildDeltaChain(checkpoints) → DeltaChain
│   └── Order deltas by timestamp/version
│
└── validateDeltaChain(chain) → boolean
    └── Verify chain integrity
```

## 5. Checkpoint Strategy

Determines when checkpoints are created:

```typescript
class CheckpointStrategy {
  static create(config: CheckpointStrategyConfig): CheckpointStrategy;

  shouldCreateCheckpoint(trigger: CheckpointTriggerType, context: CheckpointContext): boolean;
  getNextCheckpointTrigger(context: CheckpointContext): CheckpointTriggerType | null;
}

interface CheckpointStrategyConfig {
  triggers: CheckpointTriggerType[];
  interval?: number; // For ON_INTERVAL
}
```

### Trigger Types

| Trigger Type | Timing |
|-------------|--------|
| `BEFORE_EXECUTE` | Before each execution unit |
| `AFTER_EXECUTE` | After each execution unit |
| `MANUAL` | On-demand |
| `ON_ERROR` | On error |

## 6. Hierarchy Restore

### ChildCheckpointResolver

Resolves child checkpoints for hierarchical restoration:

```
ChildCheckpointResolver
├── findLatestChildCheckpoint(childId, parentCheckpointId) → Promise<string?>
├── findAllChildCheckpoints(parentExecutionId) → Promise<ChildCheckpointInfo[]>
└── resolveChildCheckpointChain(childId, depth) → Promise<string[]>
```

### ChildCheckpointRestorer

Restores child executions from checkpoints:

```
ChildCheckpointRestorer
├── restoreChildExecutions(parentEntity, checkpointDependencies) → Promise<void>
│   ├── Find child executions via ExecutionHierarchyRegistry
│   ├── Resolve latest checkpoint for each child
│   ├── Restore each child entity
│   ├── Rebuild parent-child relationships
│   └── Register restored children
│
└── restoreChildExecution(childId, childType, dependencies) → Promise<IExecutionEntity>
```

### RestoreStrategyRegistry

Registry of restore strategies for different entity types:

```
RestoreStrategyRegistry
├── registerStrategy(entityType, strategy) → void
├── getStrategy(entityType) → RestoreStrategy?
├── hasStrategy(entityType) → boolean
└── removeStrategy(entityType) → void
```

## 7. Checkpoint Metrics

### CheckpointMetricsCollector

Tracks checkpoint performance metrics:

```
CheckpointMetricsCollector
├── recordCreation(metrics) → void
├── recordRestoration(metrics) → void
├── recordError(errorContext) → void
├── getMetrics() → CheckpointMetricsReport
└── createSnapshot() → CheckpointMetricsSnapshot
```

## 8. Checkpoint Error Handling

### CheckpointErrorHandler

Handles checkpoint-related errors:

```
CheckpointErrorHandler
├── handleCheckpointError(error, context) → void
├── isRecoverableError(error) → boolean
├── getRecoveryAction(error) → RecoveryAction
└── createErrorContext(error, checkpointId) → CheckpointErrorContext
```

### RecoveryTransaction

Provides transactional recovery for checkpoint operations:

```
RecoveryTransaction
├── begin() → void
├── commit() → Promise<void>
├── rollback() → Promise<void>
└── executeWithRecovery<T>(operation) → Promise<T>
```

## 9. Supporting Utilities

### Metadata Builder

```typescript
function buildCheckpointMetadata(config: {
  entityType: string;
  entityId: string;
  formatVersion: number;
  triggerType: CheckpointTriggerType;
}): CheckpointMetadata
```

### Delta Calculator

```typescript
class DeltaCalculator {
  calculateDelta<T extends object>(base: T, current: T): Partial<T>;
  applyDelta<T extends object>(base: T, delta: Partial<T>): T;
}
```

### Cleanup Policy

```typescript
interface CleanupPolicy {
  maxCheckpoints?: number;
  maxAge?: number;
  storageLimit?: number;
}
```

### Checkpoint Cache

```typescript
class CheckpointCache {
  get(key: string): Promise<Checkpoint | null>;
  set(key: string, checkpoint: Checkpoint, ttl?: number): Promise<void>;
  invalidate(key: string): Promise<void>;
  clear(): Promise<void>;
}
```