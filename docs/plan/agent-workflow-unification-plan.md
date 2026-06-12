# Agent vs Workflow Execution Unification Plan

> Based on actual source code review of all referenced files.

## Executive Summary

The Agent Loop and Workflow execution systems share ~60% of their architectural patterns but have diverged in key areas. This plan unifies them across 4 phases plus an assessment phase, ordered by risk and impact.

**Critical correction from prior plan**: Phase 2 direction is reversed — the Workflow pattern (externalized ConversationSession) is the target. Agent should migrate toward it, not the other way around.

**Key constraint**: Agent's `AgentLoopRuntimeConfig` contains non-serializable callbacks, which fundamentally limits how checkpoint restoration can work for agent loops. This asymmetry is permanent.

---

## 0. Pre-Assessment: What Cannot Be Unified

Before investing in unification, these inherent asymmetries must be accepted:

| Aspect | Agent | Workflow | Unifiable? |
|--------|-------|----------|------------|
| Config storage | `AgentLoopRuntimeConfig` with callbacks (non-serializable) | `WorkflowExecution` data object (fully serializable) | **No** — architectural constraint |
| Checkpoint restore | Entity reuses existing config; state restored into it | Entire entity reconstructed from checkpoint | **No** — different object lifecycle |
| Execution model | Iteration loop (LLM → tools → LLM → ...) | Graph navigation (node-by-node) | **No** — fundamental paradigm difference |
| Steering/follow-up | Built-in queues for mid-execution message injection | Not applicable | **No** — agent-specific feature |
| Pause timeout | Not implemented | `PauseTimeoutManager` with force-status | **No** — workflow-specific |
| SyncBarrier | Not applicable | Fork/join synchronization primitive | **No** — workflow-specific |

**Conclusion**: Unification targets infrastructure patterns (checkpoint lifecycle, state transitions, conversation management), not execution semantics.

---

## 1. Current Architecture Issues

### 1.1 Checkpoint System (Critical — Phase 1)

| Component | Agent | Workflow |
|-----------|-------|----------|
| File | `agent/checkpoint/checkpoint-coordinator.ts` (396 lines) | `workflow/checkpoint/checkpoint-coordinator.ts` (924 lines) |
| Pattern | Extends `BaseCheckpointCoordinator` (template method) | Static methods, duplicates base logic |
| `determineCheckpointType` | `(count + 1) % baselineInterval === 0` | `count % baselineInterval === 0` |
| `extractState` | 10 lines (status, iteration, tool calls) | 74 lines (variables, node results, messages, triggers, fork/join, operations) |
| `restoreFromCheckpoint` return | `AgentLoopEntity` | `{ entity, stateCoordinator, conversationManager }` |

**Problems**:
1. Workflow re-implements `createCheckpoint`, `restoreFromCheckpoint`, `validateCheckpoint` — all already in the base class
2. Workflow's `CheckpointDependencies` has 7 fields vs base's 4 — workflow-specific deps (`workflowGraphRegistry`, `stateCoordinatorMap`, `fileCheckpointManager`, `hierarchyRegistry`) must be preserved
3. **Breaking issue**: Workflow's `restoreFromCheckpoint` returns a 3-tuple, not just the entity. Migrating to the base class template would return only `TEntity`. All callers must be updated.

### 1.2 Conversation Management (High — Phase 2)

| Aspect | Agent | Workflow |
|--------|-------|----------|
| Where ConversationSession lives | Private field in `AgentLoopEntity` | External `WorkflowStateCoordinator` |
| Access pattern | `entity.getConversationManager()` | `stateCoordinator.getConversationManager()` |
| Checkpoint serialization | **Skipped** — messages not in snapshot | **Included** — full `conversationState` in snapshot |
| External consumers | 12 production call sites | Via `WorkflowStateCoordinator` wrapper |

**Problems**:
1. Agent's `extractState()` explicitly excludes messages from checkpoint snapshots. This means agent loops **cannot be restored from checkpoint with full message history** — a functional gap.
2. 12 production call sites access `entity.getConversationManager()` directly, creating tight coupling.
3. The `ConversationCoordinator` (agent) partially duplicates what `WorkflowStateCoordinator` does.

### 1.3 StateTransitor Duplication (Medium — Phase 4)

| Component | Agent | Workflow |
|-----------|-------|----------|
| File | `agent-loop-state-transitor.ts` (276 lines) | `workflow-state-transitor.ts` (572 lines) |
| Methods | `start`, `pause`, `resume`, `complete`, `fail`, `cancel` | Same 6 + `cascadeCancel`, `cleanupChildAgentLoops`, `waitForChildExecutionsCompletion` |
| Validation | Inline status checks | `validateTransition()` utility |
| Event pattern | Dedicated builder functions | Same pattern |

**Problem**: Workflow's transitor has 3x the code because of child execution management and cascade operations. The common interface would cover only the basic 6 transitions.

---

## 2. Unification Plan

### Phase 0: API Layer Impact Assessment (Week 0)

**Goal**: Map all API-layer call sites that would be affected by Phases 1-4.

**Why first**: The SDK's API layer (`sdk/api/`) wraps internal checkpoint, entity, and lifecycle operations. Any internal API change must be assessed for external API impact before implementation begins.

**Deliverable**: A mapping document of:
- All public API methods that expose checkpoint operations
- All public API methods that expose conversation/message operations
- All public API methods that expose lifecycle operations
- Consumer impact assessment for each change

---

### Phase 1: Checkpoint System Unification

**Priority**: Critical | **Effort**: High (3-4 weeks) | **Risk**: High

**Goal**: Make `CheckpointCoordinator` extend `BaseCheckpointCoordinator`, preserving all workflow-specific restoration logic.

#### Changes Required

1. **Refactor `CheckpointCoordinator`** from static class to instance class extending `BaseCheckpointCoordinator<Checkpoint, WorkflowExecutionEntity, WorkflowExecutionStateSnapshot>`

2. **Implement abstract methods**:
   - `extractState(entity)` — extract workflow state snapshot (currently ~74 lines, stays mostly the same)
   - `buildCheckpoint(...)` — construct FULL or DELTA checkpoint (already follows template pattern)
   - `extractParentId(checkpoint)` — return `checkpoint.executionId`
   - `createEntityFromSnapshot(parentId, snapshot)` — **this is the hard part**

3. **Handle `createEntityFromSnapshot` complexity**: Currently `restoreFromCheckpoint()` does 20 steps including entity creation, variable restoration, ConversationSession creation, trigger state, FORK/JOIN inference, hierarchy validation, parent-child re-establishment, and registry registration. These must be refactored into:
   - `createEntityFromSnapshot()` — handles steps 4-14 (entity construction + state restoration)
   - Post-restore hooks or a `postRestore()` method — handles steps 15-20 (registry registration, file checkpoint, hierarchy validation)

4. **Resolve return type mismatch**: The base class returns `TEntity` (`WorkflowExecutionEntity`). Current workflow callers expect `{ entity, stateCoordinator, conversationManager }`. Options:
   - **Option A (preferred)**: Change all workflow callers to extract what they need from the entity separately. `ConversationSession` would be obtained via `WorkflowStateCoordinator` which is stored alongside the entity.
   - **Option B**: Override `restoreFromCheckpoint()` in `CheckpointCoordinator` with the wider return type. This breaks the template method pattern.

5. **Convert convenience methods** to instance methods that delegate to `createCheckpoint()`.

6. **Maintain backward compatibility**: Keep static method aliases for all public API methods.

#### Files Modified

| Action | File | Notes |
|--------|------|-------|
| Refactor | `workflow/checkpoint/checkpoint-coordinator.ts` | Core changes |
| Modify | `workflow/execution/coordinators/workflow-lifecycle-coordinator.ts` | Update restore call |
| Modify | `sdk/api/workflow/resources/*` | Update any API wrappers |
| Add test | `workflow/checkpoint/__tests__/checkpoint-coordinator-refactored.test.ts` | Regression tests |

#### Key Risk: Workflow-Specific Restoration Steps

The 20-step restore process includes critical workflow-specific logic that cannot be templated:
- Step 8-10: `ConversationSession` creation and message restoration
- Step 12: Trigger state restoration
- Step 13: FORK/JOIN context restoration
- Step 14: Triggered sub-workflow context
- Step 15: Operation state restoration
- Step 16: FORK/JOIN completion inference (queries registry)
- Step 17: Hierarchy integrity validation
- Step 18-19: Parent-child re-establishment and registry registration
- Step 20: File checkpoint restoration

**Recommendation**: Implement these as a `postRestore(entity, checkpoint)` extension method called by the subclass after the base class completes its template flow.

---

### Phase 2: Conversation Management Externalization

**Priority**: High | **Effort**: High (3-4 weeks) | **Risk**: Medium-High

**Goal**: Externalize `ConversationSession` from `AgentLoopEntity` to an `AgentStateCoordinator`, matching the workflow pattern.

#### Changes Required

1. **Create `AgentStateCoordinator`** (modeled after `WorkflowStateCoordinator`):
   - Wraps `ConversationSession`
   - Provides message access methods
   - Manages state snapshots for checkpoint

2. **Remove `ConversationSession` from `AgentLoopEntity`**:
   - Remove private `conversationManager` field
   - Remove `getConversationManager()` and `setConversationManager()`
   - Remove `addMessage()`, `getMessages()` delegation methods

3. **Update `AgentLoopCoordinator`**:
   - Create and own `AgentStateCoordinator` instance
   - Pass it to entity operations as needed (or make entity reference it via interface)

4. **Update `AgentLoopCheckpointCoordinator`**:
   - `extractState()` must now include conversation state (messages, markMap, token usage) — currently excluded
   - `createEntityFromSnapshot()` must coordinate with `AgentStateCoordinator` for message restoration

5. **Update all 12 call sites** that access `entity.getConversationManager()`:

   | File | Call Pattern | Change |
   |------|-------------|--------|
   | `agent-loop-executor.ts` (3 calls) | Real-time message access during execution | Route through coordinator |
   | `context-builder.ts` (2 calls) | Building hook context | Pass conversation session via context |
   | `agent-loop-lifecycle.ts` (2 calls) | Cloning entity | Clone via coordinator |
   | `message-resource-api.ts` (1 call) | API token stats | Route through coordinator |
   | `agent-loop-factory.ts` (3 calls) | Initialization and setup | Coordinator creation in factory |
   | `conversation-coordinator.ts` (1 call) | Delegation | Route through new coordinator |

6. **Consider the fate of `ConversationCoordinator`**: It currently wraps `AgentLoopEntity.getConversationManager()`. May need to be merged into `AgentStateCoordinator` or adapted.

#### Key Risk: Execution-Time Access Pattern

During agent loop execution, the `AgentLoopExecutor` accesses `ConversationSession` directly from the entity for:
- Getting messages for LLM context
- Adding tool response messages
- Streaming message updates

This real-time access pattern is more latency-sensitive than workflow's pattern. The `AgentStateCoordinator` must support the same access patterns without adding indirection overhead.

#### Key Risk: Checkpoint Format Change

Agent checkpoints currently do NOT serialize messages. After this change, they must. This means:
- Old checkpoints cannot be restored with the new code (missing messages)
- Version-aware restoration is needed
- The `AgentLoopStateSnapshot` type must be extended

---

### Phase 3: Entity Interface Unification

**Priority**: Medium | **Effort**: Low (1-2 weeks) | **Risk**: Low

**Goal**: Define shared behavioral contract for common entity operations.

#### Changes Required

1. **Define `ExecutionEntity` interface** with common operations:
   ```typescript
   interface ExecutionEntity {
     id: string;
     getStatus(): ExecutionStatus;
     getAbortSignal(): AbortSignal;
     interrupt(type: 'PAUSE' | 'STOP'): void;
     resetInterrupt(): void;
     cleanup(): void;
     getHierarchyMetadata(): ExecutionHierarchyMetadata | undefined;
     setParentContext(ctx: ParentExecutionContext): void;
     registerChild(ref: ChildExecutionReference): void;
   }
   ```

2. **Both `AgentLoopEntity` and `WorkflowExecutionEntity` already implement these methods** — verify and formalize.

3. **Document agent-specific extensions**:
   - `getConversationManager()` / steering queues (not part of shared interface)
   - `config` property (non-serializable, agent-only)

4. **Document workflow-specific extensions**:
   - `VariableManager` / `SyncBarrier` (not part of shared interface)
   - `getWorkflowId()` / `getGraph()`

#### Files Modified

| Action | File | Notes |
|--------|------|-------|
| Create | `core/types/execution-entity.ts` | Shared interface |
| Modify | `agent/entities/agent-loop-entity.ts` | Implement interface |
| Modify | `workflow/entities/workflow-execution-entity.ts` | Implement interface |

---

### Phase 4: StateTransitor Unification

**Priority**: Medium | **Effort**: Low (1-2 weeks) | **Risk**: Low

**Goal**: Define shared `StateTransitor` interface for basic state transitions.

#### Changes Required

1. **Define `StateTransitor` interface**:
   ```typescript
   interface StateTransitor<TEntity> {
     start(entity: TEntity): Promise<void>;
     pause(entity: TEntity): Promise<void>;
     resume(entity: TEntity): Promise<void>;
     cancel(entity: TEntity, reason?: string): Promise<void>;
     complete(entity: TEntity, result?: unknown): Promise<void>;
     fail(entity: TEntity, error: unknown): Promise<void>;
   }
   ```

2. **Have both transitors implement the interface**:
   - `AgentLoopStateTransitor` — straightforward, already matches pattern
   - `WorkflowStateTransitor` — already has matching methods, but also has 10+ additional methods for cascade operations, child management, and waiting

3. **Extract common event emission logic** into shared utility:
   - Both emit started/paused/resumed/completed/failed/cancelled events
   - Event builders already follow similar patterns

#### Key Observation

The workflow `StateTransitor` (572 lines) is 2x the agent version (276 lines) due to:
- `cascadeCancel()` — cancels all child executions with timeout/strategy options
- `cleanupChildAgentLoops()` — cleans up mixed hierarchy children
- `getChildExecutionsStatus()` — queries child statuses
- `hasActiveChildExecutions()` — checks for active children
- `waitForChildExecutionsCompletion()` — event-driven waiting
- `getWorkflowExecutionTreeDepth()` / `getAllDescendantExecutionIds()` — tree queries

These are workflow-specific and should NOT be unified. The shared interface covers only the 6 basic transitions.

---

## 3. Implementation Sequence

```
Phase 0: API Impact Assessment (2-3 days)
├── Map all public API methods affected by changes
├── Identify external consumers
└── Define backward compatibility strategy

Phase 1: Checkpoint System (3-4 weeks)
├── 1.1 Refactor CheckpointCoordinator to extend BaseCheckpointCoordinator
├── 1.2 Implement abstract methods (extractState, buildCheckpoint, etc.)
├── 1.3 Handle restoreFromCheckpoint return type mismatch
├── 1.4 Convert convenience methods to instance methods
├── 1.5 Add postRestore() extension hook for workflow-specific logic
├── 1.6 Maintain static aliases for backward compatibility
└── 1.7 Full regression test suite

Phase 2: Conversation Management (3-4 weeks)
├── 2.1 Create AgentStateCoordinator (modeled after WorkflowStateCoordinator)
├── 2.2 Update AgentLoopCoordinator to own AgentStateCoordinator
├── 2.3 Remove ConversationSession from AgentLoopEntity
├── 2.4 Update AgentLoopCheckpointCoordinator to include messages in snapshot
├── 2.5 Migrate all 12 call sites from entity.getConversationManager()
├── 2.6 Update ConversationCoordinator or integrate into AgentStateCoordinator
├── 2.7 Add version-aware checkpoint restoration (old vs new format)
└── 2.8 Full regression test suite

Phase 3: Entity Interfaces (1-2 weeks)
├── 3.1 Define ExecutionEntity shared interface
├── 3.2 Verify both entities implement all methods
├── 3.3 Add type tests for exported interfaces
└── 3.4 Update documentation

Phase 4: StateTransitor Unification (1-2 weeks)
├── 4.1 Define StateTransitor interface with 6 basic methods
├── 4.2 Update AgentLoopStateTransitor to implement interface
├── 4.3 Update WorkflowStateTransitor to implement interface
├── 4.4 Extract shared event emission utility
└── 4.5 Run tests
```

---

## 4. Risk Assessment

| Risk | Phase | Probability | Impact | Mitigation |
|------|-------|-------------|--------|------------|
| Workflow checkpoint return type breaks all callers | 1 | High | High | Static aliases + phased rollout; update all callers in same PR |
| Agent message checkpoint format change breaks old restores | 2 | High | Medium | Version-aware deserialization; fallback to empty messages for old checkpoints |
| AgentStateCoordinator introduces latency in hot execution path | 2 | Medium | High | Benchmark before/after; keep ConversationSession reference on entity if needed |
| ConversationCoordinator redundancy/confusion with AgentStateCoordinator | 2 | Medium | Low | Merge ConversationCoordinator into AgentStateCoordinator |
| StateTransitor interface too thin to be useful | 4 | Medium | Low | Accept thin interface; don't force unification of cascade/wait operations |
| Fork/JOIN state inference doesn't fit template method | 1 | High | High | Keep as post-restore hook; don't try to template it |

---

## 5. Success Criteria

1. **Phase 1**: `CheckpointCoordinator` extends `BaseCheckpointCoordinator`. All 20 restore steps preserved via `createEntityFromSnapshot` + `postRestore`. Static aliases maintain backward compatibility. All existing tests pass.

2. **Phase 2**: `AgentLoopEntity` no longer holds `ConversationSession`. Agent checkpoints serialize message state (enabling full restore). All 12 call sites migrated. Agent loop execution behavior unchanged.

3. **Phase 3**: `ExecutionEntity` interface defined and implemented by both entities. Type tests pass. No behavioral regressions.

4. **Phase 4**: `StateTransitor` interface defined and implemented by both transitors. Workflow-specific methods remain in `WorkflowStateTransitor` only.

---

## Appendix A: File Reference

| File | Lines | Phase | Role |
|------|-------|-------|------|
| `core/checkpoint/base-checkpoint-coordinator.ts` | 298 | 1 | Template method base class |
| `core/checkpoint/types.ts` | 98 | 1 | Shared types (`CheckpointableEntity`, `CheckpointDependencies`) |
| `core/checkpoint/base-diff-calculator.ts` | 160 | 1 | Generic delta calculation |
| `core/checkpoint/base-delta-restorer.ts` | 184 | 1 | Delta chain restoration |
| `agent/checkpoint/checkpoint-coordinator.ts` | 396 | 1 | Reference implementation (already extends base) |
| `workflow/checkpoint/checkpoint-coordinator.ts` | 924 | 1 | **Target of refactoring** |
| `agent/entities/agent-loop-entity.ts` | 925 | 2 | Holds `ConversationSession` (to be removed) |
| `workflow/entities/workflow-execution-entity.ts` | 873 | 2 | Externalized pattern (target) |
| `workflow/state-managers/workflow-state-coordinator.ts` | 357 | 2 | Target pattern for `AgentStateCoordinator` |
| `agent/execution/coordinators/agent-loop-coordinator.ts` | 630 | 2 | Will own `AgentStateCoordinator` |
| `agent/execution/coordinators/conversation-coordinator.ts` | 100 | 2 | May be merged into `AgentStateCoordinator` |
| `agent/execution/coordinators/agent-loop-state-transitor.ts` | 276 | 4 | Agent state transitions |
| `workflow/execution/coordinators/workflow-state-transitor.ts` | 572 | 4 | Workflow state transitions |
| `core/messaging/conversation-session.ts` | — | 2 | Shared `ConversationSession` |

## Appendix B: Key Insights from Code Review

1. **Workflow `restoreFromCheckpoint` constructs a new entity** — unlike Agent which restores state into an existing entity. The base class template returns `TEntity`, so workflow's 3-tuple return is incompatible without overriding.

2. **Agent `determineCheckpointType` is already different from base** — uses `(count + 1) % baselineInterval` for iteration alignment. This must be preserved as an override when migrating to base class.

3. **Workflow `extractState` is complex by nature** — not duplicated code. It extracts variables, node results, messages, triggers, fork/join, and operation state. This is workflow-specific and stays in the subclass.

4. **Agent checkpoint currently does NOT serialize messages** — a functional gap. Externalizing ConversationSession enables fixing this.

5. **`ConversationCoordinator` (agent) partially duplicates `WorkflowStateCoordinator`** — the agent already has a thin coordinator wrapper. Merging or replacing it should be considered in Phase 2.

6. **Both transitors share identical event emission patterns** — the 6 basic transitions emit started/paused/resumed/completed/failed/cancelled events using the same builder pattern. Shared utility is feasible.

7. **Agent `AgentLoopCheckpointDependencies` in lifecycle handler uses `unknown` types** — this is a pre-existing type safety gap that should be fixed as part of Phase 1.