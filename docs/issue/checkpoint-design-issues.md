# Checkpoint System Design Issues

## Checkpoint Function Analysis

### Overall Architecture

The checkpoint system adopts a layered architecture:

```
API Layer (Commands & Resources)
    ↓
Coordinator Layer (Template Method: Workflow/AgentLoop CheckpointCoordinator)
    ↓
State Manager Layer (CRUD + Cleanup Strategy)
    ↓
Shared Utilities Layer (BaseDeltaRestorer, BaseDiffCalculator, ChildCheckpointRestorer)
    ↓
Storage Adapter Layer (SQLite/Memory/Postgres)
```

### Core Mechanisms

#### 1. Incremental Snapshot (Delta Checkpoint)

- **FullCheckpoint**: Stores complete state snapshot
- **DeltaCheckpoint**: Stores only `previousCheckpointId` and `delta` (change map)
- **Delta Chain Restore**: `BaseDeltaRestorer` starts from the nearest FULL checkpoint and applies DELTA sequentially to restore the complete state

#### 2. Sub-execution Instance Processing

**Recovery Flow** (`ChildCheckpointRestorer.restoreChildren`):

1. Group sub-instances by type (WORKFLOW before AGENT_LOOP)
2. Iterate through each sub-instance serially:
   - Find latest checkpoint (`findCheckpoint`)
   - Recursively restore entity (`restoreEntity`)
   - Register to parent entity (`registerChild`)
   - Recursively restore grandchild instances

**Key Files**:
- `packages/sdk/shared/checkpoint/child-checkpoint-restorer.ts`
- `packages/sdk/shared/checkpoint/child-checkpoint-resolver.ts`

---

## Design Issues Analysis

### P0 (Critical) Issues

#### 1. Sub-instance Recovery Completely Serial, No Concurrency Control

**Location**: `ChildCheckpointRestorer.restoreChildren()` L125-145

**Problem**: Uses sequential `for` loop to restore sub-instances one by one, no concurrency limits. Recovery time is O(n) × single recovery time, with no backpressure mechanism for downstream systems (storage/LLM).

#### 2. Partial Failure Causes State Inconsistency

**Location**: Workflow `postRestore` L847-853

**Problem**: When sub-instance partial recovery succeeds, the parent entity's hierarchy is not updated. The "rotten" state propagates downward. Subsequent checkpoints based on this parent state will record incomplete hierarchy.

**Impact**: Data corruption propagates through checkpoint chains.

#### 3. Cleanup Strategy Lacks Dependency Awareness

**Location**: `BaseCheckpointStateManager.executeCleanupForEntityInternal`

**Problem**: Time-based cleanup may delete the FULL checkpoint in the middle of a delta chain, breaking subsequent DELTA recovery. `SizeBasedCleanupStrategy` doesn't consider DELTA dependencies, potentially deleting critical FULL base checkpoints.

**Impact**: Chain breakage makes delta checkpoints unrecoverable.

#### 4. Deep Recursion Without Stack Overflow Protection

**Location**: `ChildCheckpointRestorer.restoreChildren` → `restoreSingleChild` → `restoreChildren` recursive calls

**Problem**: No tail recursion optimization protection. When hierarchy depth reaches thousands of levels (nested sub-workflows), JavaScript throws `RangeError`.

#### 5. Cross-type Recovery Design Coupling

**Location**: `buildChildRestoreDependencies` and `AgentCheckpointDependencies`

**Problem**: Workflow/Agent sub-recovery has asymmetric config acquisition methods. Agent uses `this.restoreConfig` from coordinator instance state, while Workflow fetches from `WorkflowCheckpointDependencies`. This hidden coupling makes cross-type recovery fragile.

#### 6. No Transaction Protection Between Checkpoint Creation and Recovery

**Location**: Entire checkpoint creation → save → recovery flow

**Problem**: If `createCheckpoint` succeeds but subsequent sub-entity recovery partially fails, the database stores a checkpoint referencing incomplete subsets. Next recovery will reproduce the same inconsistency.

---

### P1 (Important) Issues

#### 7. N+1 Query Problem Amplified in Sub-restore

**Location**: `ChildCheckpointRestorer.restoreSingleChild` + `StorageBackedChildResolver`

**Problem**: Restoring N sub-instances generates 1100+ storage queries with no batching. Each sub-instance independently queries for the latest checkpoint.

#### 8. Cycle Detection Cross-branch Blind Spot

**Location**: `ChildCheckpointRestorer.restoreChildren` L107, L136, L148

**Problem**: `visited.add` executes synchronously before `await`, but recursion is asynchronous. When two different childRefs recursively restore to the same grandchild node, cycle detection fails (will be exposed if changed to parallel in future).

#### 9. Delta Chain Breakage Risk in Cleanup

**Location**: `SizeBasedCleanupStrategy`, `TimeBasedCleanupStrategy`

**Problem**: Selection logic based purely on timestamp/size without considering FULL-DELTA reference relationships. A critical FULL base might be deleted while its dependent DELTAs remain.

#### 10. Duplicate Recovery Logic

**Location**: `ExecutionRestoreCoordinator.restoreChildrenRecursive` vs `ChildCheckpointRestorer.restoreChildren`

**Problem**: Two independent implementations of sub-instance restoration with overlapping functionality scattered across different locations. Maintainers cannot unify understanding of recovery behavior.

#### 11. Hierarchy Metadata Out of Sync with Registry

**Location**: `buildChildRestoreDependencies` L982-987

**Problem**: After sub-instance recovery, parent's hierarchy metadata is not updated based on actual successfully-restored children. Subsequent checkpoints will record inconsistent child references.

#### 12. Agent Delta Lacks metadataLoader

**Location**: `agent/checkpoint/checkpoint-coordinator.ts` L274-278

**Problem**: Creates `BaseDeltaRestorer` without `metadataLoader`, forcing sequential traversal path. Each DELTA recovery requires O(chainLength) storage queries.

---

### P2 (Minor) Issues

| # | Issue | Location |
|---|-------|----------|
| 13 | `findCheckpoint` assumes ID auto-increment sorting; UUID "take latest" judgment fails | `buildChildRestoreDependencies` |
| 14 | `TieredCleanupStrategy` inner `some` is O(n²) | `cleanup-policy.ts` L96-106 |
| 15 | `CheckpointCoordinator.buildCheckpoint` independently creates `BaseDeltaRestorer`, extra allocation | `checkpoint-coordinator.ts` |
| 16 | `AgentCheckpointDependencies` has semantically confusing dual resolver fields | `agent/checkpoint/dependencies.ts` |

---

## Modification Plan

### Phase 1: Fix Critical Data Consistency Issues

#### M1.1 Implement Hierarchical Cleanup Strategy

**Goal**: Ensure delta chain integrity during cleanup

**Approach**:
1. Add `dependencyGraph` field to `CheckpointStorageMetadata` to track FULL-DELTA references
2. Before cleanup execution, build reference chain map
3. When selecting deletion candidates, upward traverse dependency chain - if FULL is referenced by any surviving DELTA, mark as protected
4. Add cascade cleanup option: when deleting parent checkpoint, optionally cascade delete child checkpoints

**Files to modify**:
- `packages/types/src/storage/checkpoint-storage.ts` - Add dependency graph types
- `packages/sdk/shared/checkpoint/base-checkpoint-state-manager.ts` - Implement dependency-aware cleanup
- `packages/storage/src/sqlite/sqlite-checkpoint-storage.ts` - Add reference tracking queries

#### M1.2 Implement Recovery Transaction Semantics

**Goal**: Ensure all-or-nothing recovery of sub-instances

**Approach**:
1. Introduce `RecoveryTransaction` Coordinator that tracks recovery progress
2. Pre-validate all sub-instance checkpoint availability before actual restore
3. Implement compensation log: on failure, release registered entities and rollback hierarchy state
4. Add `recoveryId` field to track recovery operations in events

**Files to create/modify**:
- `packages/sdk/shared/checkpoint/recovery-transaction.ts` - New transaction coordinator
- `packages/sdk/shared/checkpoint/child-checkpoint-restorer.ts` - Integrate transaction
- `packages/sdk/workflow/checkpoint/checkpoint-coordinator.ts` - Add compensation logic

#### M1.3 Fix Hierarchy Metadata Sync

**Goal**: Keep hierarchy metadata consistent with actual registry state

**Approach**:
1. After each successful child restoration, update parent's hierarchy metadata
2. Add `resolveActualHierarchy()` method to dynamically build hierarchy from registry
3. On partial failure, mark inconsistent children as `unrecoverable` in metadata
4. Subsequent checkpoint creation uses actual hierarchy, not snapshot

**Files to modify**:
- `packages/sdk/shared/checkpoint/child-checkpoint-restorer.ts` - Sync metadata after restoration
- `packages/sdk/workflow/checkpoint/checkpoint-coordinator.ts` - Use actual hierarchy for snapshot
- `packages/sdk/shared/checkpoint/hierarchy-integrity-service.ts` - Add consistency repair

---

### Phase 2: Improve Performance and Scalability

#### M2.1 Implement Parallel Recovery with Concurrency Control

**Goal**: Restore sub-instances in parallel with bounded concurrency

**Approach**:
1. Replace serial `for` loop with `p-limit` or similar concurrency-limited parallel execution
2. Set default concurrency limit (e.g., 5-10 concurrent restores)
3. Implement circuit breaker: stop parallel restores when failure rate exceeds threshold
4. Add configurable concurrency limits per entity type

**Files to modify**:
- `packages/sdk/shared/checkpoint/child-checkpoint-restorer.ts` - Replace serial loop
- `packages/sdk/shared/checkpoint/types.ts` - Add concurrency config types
- `packages/sdk/shared/utils/concurrency.ts` - New concurrency utilities (if not exists)

#### M2.2 Implement Batch Checkpoint Loading

**Goal**: Eliminate N+1 query problem

**Approach**:
1. Add `resolveLatestCheckpointsBatch(childRefs[])` method to `ChildCheckpointResolver`
2. Single query using `IN (id1, id2, ...)` clause to fetch latest checkpoints for all sub-instances
3. Pre-load entire delta chain in one batch operation
4. Implement checkpoint cache for repeated recovery of same entities

**Files to modify**:
- `packages/sdk/shared/checkpoint/child-checkpoint-resolver.ts` - Add batch resolution
- `packages/storage/src/types/adapter/checkpoint-adapter.ts` - Add batch query interface
- `packages/storage/src/sqlite/sqlite-checkpoint-storage.ts` - Implement batch queries

#### M2.3 Convert Deep Recursion to Iterative

**Goal**: Prevent stack overflow on deeply nested hierarchies

**Approach**:
1. Replace recursive `restoreChildren` with explicit stack-based iteration
2. Use `while` loop with pending stack tracking
3. Add depth counter with configurable maximum depth limit
4. Implement breadth-first restoration option for wide hierarchies

**Files to modify**:
- `packages/sdk/shared/checkpoint/child-checkpoint-restorer.ts` - Rewrite to iterative
- `packages/sdk/shared/checkpoint/types.ts` - Add depth limit config

---

### Phase 3: Improve Architecture Consistency

#### M3.1 Unify Sub-instance Recovery Entry Points

**Goal**: Eliminate duplicate recovery logic

**Approach**:
1. Make `ChildCheckpointRestorer` the single entry point for all sub-instance recovery
2. Remove `ExecutionRestoreCoordinator.restoreChildrenRecursive` functionality
3. Add `RestoreStrategy` interface for entity-type-specific recovery logic
4. Both Workflow and Agent coordinators delegate to `ChildCheckpointRestorer`

**Files to modify**:
- `packages/sdk/shared/checkpoint/child-checkpoint-restorer.ts` - Become unified entry
- `packages/sdk/shared/checkpoint/restore-strategy.ts` - New strategy interface
- `packages/sdk/workflow/checkpoint/checkpoint-coordinator.ts` - Delegate to shared restorer
- `packages/sdk/agent/checkpoint/checkpoint-coordinator.ts` - Delegate to shared restorer

#### M3.2 Unify Cross-type Recovery Configuration

**Goal**: Eliminate asymmetric config acquisition

**Approach**:
1. Define `SubRestoreConfig` interface with all necessary dependencies
2. Both Workflow and Agent sub-receive same config structure
3. Remove `restoreConfig` instance state from Agent coordinator
4. Config explicitly passed through dependency chain

**Files to modify**:
- `packages/sdk/shared/checkpoint/types.ts` - Add unified config types
- `packages/sdk/workflow/checkpoint/checkpoint-coordinator.ts` - Refactor dependency building
- `packages/sdk/agent/checkpoint/checkpoint-coordinator.ts` - Remove instance state dependency

#### M3.3 Add metadataLoader to Agent Delta Restorer

**Goal**: Enable optimized delta chain traversal for Agent

**Approach**:
1. Pass `metadataLoader` when creating `BaseDeltaRestorer` in Agent coordinator
2. Reuse same metadata loading logic as Workflow
3. Add `chainRootId` optimization to Agent delta chain

**Files to modify**:
- `packages/sdk/agent/checkpoint/checkpoint-coordinator.ts` - Add metadataLoader parameter

---

### Phase 4: Defensive Programming Improvements

#### M4.1 Enhance Cycle Detection

**Goal**: Prevent infinite loops in corrupted hierarchies

**Approach**:
1. Move `visited.add` after successful restoration (not before)
2. Use `WeakSet` for object-level tracking in addition to ID tracking
3. Add path-based cycle detection logging for debugging
4. Implement recovery checkpoint for partial progress

**Files to modify**:
- `packages/sdk/shared/checkpoint/child-checkpoint-restorer.ts` - Fix cycle detection timing

#### M4.2 Fix Checkpoint Latest Selection Logic

**Goal**: Correctly identify latest checkpoint regardless of ID format

**Approach**:
1. Sort by `timestamp` field instead of ID position
2. Add composite index `(entityType, entityId, timestamp DESC)` to storage
3. Update `findCheckpoint` to use timestamp ordering

**Files to modify**:
- `packages/sdk/shared/checkpoint/child-checkpoint-resolver.ts` - Sort by timestamp
- `packages/storage/src/sqlite/sqlite-checkpoint-storage.ts` - Add composite index

#### M4.3 Add Checkpoint Metrics and Monitoring

**Goal**: Observability for checkpoint operations

**Approach**:
1. Add metrics for: recovery duration, chain length, failure count, cleanup effectiveness
2. Emit warnings when delta chain exceeds threshold
3. Track hierarchy depth distribution
4. Add health check endpoint for checkpoint system

**Files to modify**:
- `packages/sdk/shared/checkpoint/metrics.ts` - Add new metrics
- `packages/sdk/shared/checkpoint/base-checkpoint-state-manager.ts` - Instrument key operations

---

## Implementation Priority

| Priority | Items | Expected Impact |
|----------|-------|-----------------|
| **P0 (Immediate)** | M1.1, M1.2, M1.3 | Fix data corruption risks |
| **P1 (Next Sprint)** | M2.1, M2.2, M2.3 | Improve performance and scalability |
| **P2 (Following Sprint)** | M3.1, M3.2, M3.3 | Improve code maintainability |
| **P3 (Ongoing)** | M4.1, M4.2, M4.3 | Defensive improvements |

---

## Testing Strategy

### Unit Tests

1. **Cleanup Dependency Tests**
   - Verify FULL checkpoint protected when DELTA references exist
   - Verify cascade cleanup removes child checkpoints
   - Verify cleanup watermark updates correctly

2. **Recovery Transaction Tests**
   - Verify all-or-nothing recovery behavior
   - Verify compensation rollback on partial failure
   - Verify hierarchy metadata consistency after recovery

3. **Parallel Recovery Tests**
   - Verify concurrency limit enforcement
   - Verify circuit breaker triggers on high failure rate
   - Verify parallel recovery produces same result as serial

4. **Cycle Detection Tests**
   - Verify detection of direct cycles (A → A)
   - Verify detection of indirect cycles (A → B → A)
   - Verify recovery continues after cycle detected

### Integration Tests

1. **End-to-end Recovery Tests**
   - Create nested workflow → checkpoint → restore → verify hierarchy
   - Simulate partial failure → verify rollback
   - Large hierarchy (100+ sub-instances) → verify performance

2. **Concurrent Checkpoint Tests**
   - Multiple simultaneous checkpoint creations
   - Checkpoint creation during recovery
   - Cleanup during active recovery

3. **Storage Failure Tests**
   - Storage unavailable during delta chain traversal
   - Corrupted checkpoint data detection
   - Partial write failures
