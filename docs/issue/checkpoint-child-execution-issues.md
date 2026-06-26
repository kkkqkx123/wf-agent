# Checkpoint Child Execution Instance Issues and Resolution

## 1. Architecture Overview

```
Checkpoint System Workflow:
┌─────────────────────────────────────────────────────────────┐
│  BaseCheckpointCoordinator (模板方法)                       │
│  ├── extractState() → snapshot                              │
│  ├── determineCheckpointType() → FULL/DELTA                 │
│  ├── buildCheckpoint()                                      │
│  └── restoreFromCheckpoint()                                │
├──────────────────────┬──────────────────────────────────────┤
│ Workflow Coordinator │ Agent Coordinator                    │
│ - restoreWorkflow()  │ - restoreAgentLoop()                 │
│ - 使用共享组件恢复子实例                                     │
└──────────────────────┴──────────────────────────────────────┘

Shared Components:
┌─────────────────────────────────────────────────────────────┐
│  ChildRestoreComponent (packages/sdk/shared/checkpoint/)       │
│  - 跨类型子实例恢复                                          │
│  - WORKFLOW 先于 AGENT_LOOP 恢复                             │
│  - 错误隔离、循环检测                                        │
└─────────────────────────────────────────────────────────────┘

Delta Chain Structure:
[FULL:A] ← [DELTA:B] ← [DELTA:C] ← [FULL:D] ← [DELTA:E]
  ↑                                     ↑
chainRootId                          chainPosition=0
```

## 2. Issues Identified

| ID | Issue | Location | Impact |
|----|-------|----------|--------|
| BUG-1 | Agent `createCheckpoint` signature mismatch | `agent/checkpoint-coordinator.ts:141` | `contentConfig` not passable |
| BUG-2 | Set comparison always fails for objects | `base-diff-calculator.ts:214` | Delta calculation incorrect for object Sets |
| BUG-3 | SQLite `variableCount` always 0 | `sqlite-checkpoint-storage.ts:165` | Metadata stats inaccurate |
| GAP-1 | Child restoration not symmetric | Both coordinators | Workflow ignores AGENT_LOOP children |
| GAP-2 | No generic child checkpoint resolver | Both coordinators | Assumes list ordering for latest |
| ROB-1 | Cleanup watermark race condition | `base-checkpoint-state-manager.ts:384-403` | Concurrent cleanup may orphan chains |
| ROB-2 | `agentLoopId` fallback to constructor value | `agent/checkpoint-state-manager.ts:136` | Wrong checkpoint may corrupt chain |

## 3. Fix Plan

### 3.1 BUG-1: Agent Coordinator Signature Mismatch

**Root cause**: Agent coordinator's `createCheckpoint` override omits `context` parameter.

**Fix**: Add optional 4th parameter `context?: CheckpointContentContext`.

**Files**: `packages/sdk/agent/checkpoint/checkpoint-coordinator.ts:141`

### 3.2 BUG-2: Set Comparison Logic Error

**Root cause**: `deepEqual(item, item)` is always true, making condition always false.

**Fix**: Check `Set.has()` for referential equality first, then iterate with `deepEqual` for object elements.

**Files**: `packages/sdk/shared/checkpoint/diff/base-diff-calculator.ts:214`

### 3.3 BUG-3: SQLite variableCount Calculation

**Root cause**: Snapshot uses `Record<string, unknown>`, not array. `Array.isArray` always returns false.

**Fix**: Check `variableState.variables` (correct field) and use `Object.keys().length`.

**Files**: `packages/storage/src/implementations/sqlite-checkpoint-storage.ts:165`

### 3.4 GAP-1 + GAP-2: Symmetric Child Restoration

**Root cause**: Workflow coordinator skips AGENT_LOOP children. Both coordinators manually implement child restoration.

**Fix**: Create `ChildRestoreComponent` in `shared/checkpoint/`:
- Handles both WORKFLOW and AGENT_LOOP children
- Uses `ChildCheckpointResolver` for explicit latest-checkpoint lookup
- Isolates errors per-child (one failure doesn't block others)
- Detects cycles in hierarchy
- Restores WORKFLOW before AGENT_LOOP (dependency order)

Both coordinators delegate child restoration to this shared component:

```typescript
// Both Workflow and Agent coordinators use:
const component = new ChildRestoreComponent();
const results = await component.restoreChildren(entity, childRefs, restoreDeps);
```

**Architecture**:
```
ChildRestoreComponent (shared/checkpoint/child-restore-component.txt)
├── ChildRestoreDependencies (interfaces)
│   ├── findCheckpoint: (childId, childType) => Promise<string | undefined>
│   ├── restoreEntity: (checkpointId, childType) => Promise<IExecutionEntity>
│   └── registerChild: (parent, child, childRef) => void
└── restoreChildren() - recursive with cycle detection

ChildCheckpointResolver (shared/checkpoint/child-checkpoint-resolver.ts)
├── StorageBackedChildResolver - queries storage with ORDER BY
└── CachedChildResolver - in-memory cache for batch operations
```

**Files**:
- New: `packages/sdk/shared/checkpoint/child-restore-component.ts`
- New: `packages/sdk/shared/checkpoint/child-checkpoint-resolver.ts`
- Modified: `packages/sdk/workflow/checkpoint/checkpoint-coordinator.ts`
- Modified: `packages/sdk/agent/checkpoint/checkpoint-coordinator.ts`

### 3.5 ROB-1: Cleanup Watermark Race Condition

**Root cause**: Read-modify-write without transaction in application code.

**Fix**: Add per-entity lock using promise chain serialization (`withEntityLock` method).

**Files**: `packages/sdk/shared/checkpoint/base-checkpoint-state-manager.ts:384-403`

### 3.6 ROB-2: agentLoopId Validation

**Root cause**: `checkpoint.agentLoopId || this.agentLoopId` may silently mask mismatches.

**Fix**: Log warning when checkpoint's `agentLoopId` differs from manager binding.

**Files**: `packages/sdk/agent/checkpoint/checkpoint-state-manager.ts:136`

## 4. Implementation Order

| Phase | Items | Dependency |
|-------|-------|------------|
| **P0 - Critical Fixes** | BUG-1, BUG-2, BUG-3 | None |
| **P1 - Architecture** | GAP-1, GAP-2 (ChildRestoreComponent) | P0 complete |
| **P2 - Robustness** | ROB-1, ROB-2 | P1 complete |

## 5. Files Impact Summary

| File | Change Type |
|------|-------------|
| `sdk/agent/checkpoint/checkpoint-coordinator.ts` | Modify |
| `sdk/agent/checkpoint/checkpoint-state-manager.ts` | Add validation, logging |
| `sdk/workflow/checkpoint/checkpoint-coordinator.ts` | Modify |
| `sdk/shared/checkpoint/base-diff-calculator.ts` | Fix |
| `sdk/shared/checkpoint/base-checkpoint-state-manager.ts` | Add withEntityLock |
| `sdk/shared/checkpoint/child-restore-component.ts` | New |
| `sdk/shared/checkpoint/child-checkpoint-resolver.ts` | New |
| `storage/src/sqlite/sqlite-checkpoint-storage.ts` | Fix |

## 6. Design Decisions

### 6.1 Why ChildRestoreComponent in shared/?

Child restoration is a cross-type concern:
- Both Workflow and Agent need to restore children of either type
- The logic is identical regardless of parent type
- Placing it in agent/ would create asymmetric dependency

### 6.2 Restoration Order

WORKFLOW children are restored before AGENT_LOOP children because:
- Agent nodes may depend on workflow execution state
- Workflow outputs may be agent inputs

### 6.3 Error Isolation

One child failure doesn't block others:
- Each child restoration is wrapped in try-catch
- Failures are collected and reported after all children processed
- Hierarchy validation runs after restoration to detect any orphan references

### 6.4 Why not ChildExecutor interface?

Original design used a `ChildExecutor` interface injected into Agent coordinator. This was rejected because:
- It placed cross-type knowledge in the agent module
- Violated architectural layering (SDK knows about apps)
- The shared component approach is symmetric and doesn't require interface injection
