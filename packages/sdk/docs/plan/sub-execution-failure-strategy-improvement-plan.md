# Sub-Execution Failure Strategy Improvement Plan

## Overview

This document proposes a systematic improvement plan for how the SDK handles sub-execution instance failures across all execution types: SUBGRAPH, FORK, JOIN, Agent Loop, and Workflow LOOP.

## Background

The current SDK implements sub-execution failure handling inconsistently across different execution types. The most critical issue is the Fork handler's `Promise.all` fast-fail behavior, which prevents the existing Join strategies from working effectively. A detailed analysis is available in the [analysis report](../../analysis_report.md).

---

## Phase 1: Fork Failure Isolation (P0)

### Problem

The Fork handler (`fork-handler.ts`) uses `Promise.all` to execute all branches in parallel. If any single branch throws an exception, `Promise.all` immediately rejects—causing:

- All other branches' results to be discarded
- The Join handler's 5 strategies (`ALL_COMPLETED`, `ANY_COMPLETED`, `ALL_FAILED`, `ANY_FAILED`, `SUCCESS_COUNT_THRESHOLD`) to be ineffective
- Resource waste from branches that continue running after the rejection

### Solution

#### 1.1 Replace `Promise.all` with `Promise.allSettled` for Fork branch execution

**File**: `packages/sdk/workflow/execution/handlers/node-handlers/fork-handler.ts`

**Current behavior** (lines 184-203):

```typescript
const executionResults = await Promise.all(
  branchCreations.map(async branch => {
    const result = await executor.executeWorkflow(branch.branchEntity);
    // ... build result with status
  }),
);
```

**Proposed behavior**:

```typescript
const settledResults = await Promise.allSettled(
  branchCreations.map(async branch => {
    const branchStartTime = now();
    try {
      const result = await executor.executeWorkflow(branch.branchEntity);
      const branchDuration = diffTimestamp(branchStartTime, now());
      return { branch, result, branchDuration, status: result.metadata.status };
    } catch (error) {
      const branchDuration = diffTimestamp(branchStartTime, now());
      return { branch, error, branchDuration, status: "FAILED" };
    }
  }),
);

const executionResults = settledResults.map(r => {
  if (r.status === "fulfilled") return r.value;
  // r.reason is already handled inside the map callback
  return r as unknown as { branch: typeof branchCreations[0]; status: "FAILED" };
});
```

This ensures that:
- A failure in one branch does NOT prevent other branches from completing
- Each branch's result is independently collected
- Failed branches are still tracked with FAILED status

#### 1.2 Add `failureStrategy` to Fork node configuration

**File**: `packages/sdk/workflow/execution/types/fork.types.ts`

Add a new type to `ForkExecutionConfig`:

```typescript
export interface ForkExecutionConfig {
  /** Fork strategy: parallel (default) or serial */
  strategy: "parallel" | "serial";
  /** Maximum concurrent branches (for resource control) */
  maxConcurrency?: number;
  /** Timeout for all branches in milliseconds */
  timeout?: number;
  /** NEW: Failure handling strategy for branches */
  failureStrategy?: "fail-fast" | "continue-on-error" | "fail-on-threshold";
  /** NEW: Maximum number of failed branches allowed (only used with fail-on-threshold) */
  maxFailedBranches?: number;
}
```

**File**: `packages/sdk/types/...` (workflow node types)

Add `failureStrategy` to `ForkNodeConfig` in the type definitions:

```typescript
export interface ForkNodeConfig extends BaseNodeConfig {
  forkPaths: ForkPath[];
  forkStrategy?: "parallel" | "serial";
  maxConcurrency?: number;
  /** NEW: Failure handling strategy */
  failureStrategy?: "fail-fast" | "continue-on-error" | "fail-on-threshold";
  /** NEW: Max failures allowed (default: 0, meaning fail-fast) */
  maxFailedBranches?: number;
}
```

#### 1.3 Apply failureStrategy in Fork handler

In `fork-handler.ts` after `Promise.allSettled`, apply the strategy:

```typescript
// After collecting all branch results
const failedBranches = executionResults.filter(r => r.status === "FAILED");

if (failureStrategy === "fail-fast" && failedBranches.length > 0) {
  // Fail immediately (same as current behavior, but with all branches completed)
  throw new Error(`Fork branches failed: ${failedBranches.map(b => b.branch.pathId).join(", ")}`);
}

if (failureStrategy === "fail-on-threshold" && failedBranches.length > (maxFailedBranches ?? 0)) {
  throw new Error(`Too many fork branches failed: ${failedBranches.length} > ${maxFailedBranches ?? 0}`);
}

// "continue-on-error": proceed regardless of failures
```

---

## Phase 2: Fork Result Integration with Join Strategy (P0)

### Problem

Currently, the Fork handler returns results as `ForkBranchResult[]` but does not persist failure information in a way the Join handler can easily consume. The Join handler relies on `collectBranches()` which reads from `SyncBarrier` + `ExecutionRegistry`, but the branch execution status needs to be persisted properly.

### Solution

#### 2.1 Ensure failed branches are registered in SyncBarrier with FAILED status

**File**: `packages/sdk/workflow/execution/handlers/node-handlers/fork-handler.ts`

After `Promise.allSettled` + strategy check, ensure each branch's execution ID is registered in the SyncBarrier with its final status so the Join handler can read it.

The current `collectBranches()` in `join-handler.ts` already distinguishes between `COMPLETED`, `FAILED`/`CANCELLED`, and skipped branches. With the `Promise.allSettled` change, the SyncBarrier will have all branches registered, and the ExecutionRegistry will have their actual statuses.

#### 2.2 Verify Join handler compatibility

**File**: `packages/sdk/workflow/execution/handlers/node-handlers/join-handler.ts`

No changes needed to `collectBranches()` or `evaluateStrategy()`—they already handle the status-based classification correctly. The existing strategies will now work as intended:

| Strategy | After fix | Before fix |
|---|---|---|
| `ALL_COMPLETED` | Works correctly | Broken (Promise.all fast-fail) |
| `ANY_COMPLETED` | Works correctly | Broken |
| `ALL_FAILED` | Works correctly | Broken |
| `ANY_FAILED` | Works correctly | Broken |
| `SUCCESS_COUNT_THRESHOLD` | Works correctly | Broken |

---

## Phase 3: Subgraph Failure Strategy (P1)

### Problem

The SUBGRAPH handler (`node-handlers/subgraph-handler.ts`) only supports fail-fast: any error during subgraph execution is caught, logged, and re-thrown—causing the parent workflow to fail.

### Solution

#### 3.1 Add `onFailure` config to SubgraphNodeConfig

**File**: `packages/sdk/types/...` (workflow node types)

```typescript
export interface SubgraphNodeConfig extends BaseNodeConfig {
  subgraphId: string;
  variableInputs?: VariableMapping[];
  variableOutputs?: VariableMapping[];
  dataInputs?: SubgraphDataInput[];
  dataOutputs?: SubgraphDataOutput[];
  /** NEW: Failure handling strategy */
  onFailure?: "fail" | "continue" | "retry";
  /** NEW: Max retry attempts (only used with retry strategy, default: 3) */
  maxRetries?: number;
  /** NEW: Retry delay in ms (default: 1000) */
  retryDelayMs?: number;
  /** NEW: Fallback output value when continuing on failure */
  fallbackOutput?: Record<string, unknown>;
}
```

#### 3.2 Apply failure strategy in Subgraph handler

**File**: `packages/sdk/workflow/execution/handlers/node-handlers/subgraph-handler.ts`

In the catch block (lines 198-253), instead of always re-throwing:

```typescript
} catch (error) {
  // ... existing logging and metrics ...

  switch (onFailure) {
    case "continue":
      logger.warn("Subgraph failed but continuing per onFailure=continue", { ... });
      return {
        executionResult: { status: "SKIPPED", output: fallbackOutput ?? {} },
        duration: executionDuration,
      };

    case "retry":
      // Retry logic
      for (let attempt = 1; attempt <= maxRetries; attempt++) {
        await delay(retryDelayMs * attempt); // exponential backoff
        try {
          const retryResult = await executor.executeWorkflow(subgraphEntity);
          // ... handle success ...
          return { executionResult: retryResult, duration: Date.now() - startTime };
        } catch (retryError) {
          // Log and continue retry loop
        }
      }
      // All retries exhausted, fall through to fail
      throw new Error(`Subgraph failed after ${maxRetries} retries...`);

    case "fail":  // default
    default:
      // Cleanup and re-throw (current behavior)
      await cleanupChildExecution(subgraphEntity, parentEntity, "FAILED");
      throw new Error(`Subgraph execution failed...`);
  }
}
```

---

## Phase 4: Loop Iteration Failure Strategy (P1)

### Problem

The LOOP node (`loop-start-handler.ts` / `loop-end-handler.ts`) does not handle iteration-internal failures. If a node inside the loop body fails, the error propagates to `NodeExecutionCoordinator` and terminates the entire workflow.

### Solution

#### 4.1 Add `onIterationFailure` to LoopStartNodeConfig

**File**: `packages/sdk/types/...`

```typescript
export interface LoopStartNodeConfig extends BaseNodeConfig {
  loopId: string;
  // ... existing fields ...
  /** NEW: Failure handling strategy for loop iterations */
  onIterationFailure?: "fail" | "skip" | "continue";
  /** NEW: Max consecutive failures before terminating (default: 0 = no limit) */
  maxConsecutiveFailures?: number;
}
```

#### 4.2 Record iteration failure state in LoopState

**File**: `packages/sdk/workflow/execution/handlers/node-handlers/loop-start-handler.ts`

Expand `LoopState`:

```typescript
interface LoopState {
  loopId: string;
  iterable: unknown | null;
  currentIndex: number;
  maxIterations: number;
  iterationCount: number;
  variableName: string | null;
  /** NEW: Track consecutive failures for skip-on-failure strategy */
  consecutiveFailures: number;
  /** NEW: Track total failures for threshold checks */
  totalFailures: number;
}
```

#### 4.3 Apply strategy in the loop iteration flow

The loop execution flow is controlled by the `WorkflowExecutionCoordinator.execute()` loop, which navigates from one node to the next. The coordination between LOOP_START → body nodes → LOOP_END is handled by the graph navigation.

To support iteration failure handling, the `NodeExecutionCoordinator.executeNode()` or the `WorkflowExecutionCoordinator.execute()` needs to be aware of the loop context. When a node inside a loop body fails:

- **`skip`**: Record the failure count, skip to LOOP_END for iteration bookkeeping, continue to next iteration
- **`continue`**: Same as success for iteration purposes, continue to next iteration
- **`fail`**: Current behavior (terminate workflow)

This requires a coordination mechanism between `NodeExecutionCoordinator` and the loop state.

---

## Phase 5: Unified Error Classification (P2)

### Problem

The Agent layer has a severity-based error classification (`error`/`warning`/`info`), but the workflow layer's sub-execution handlers do not share this classification. This leads to inconsistent error handling semantics.

### Solution

#### 5.1 Define a unified error classification for sub-execution failures

**File**: `packages/sdk/shared/types/execution.ts` (or new file)

```typescript
/** Unified severity for sub-execution failures */
export type SubExecutionFailureSeverity = 
  | "fatal"    // Terminate parent execution immediately
  | "error"    // Apply configured failure strategy
  | "warning"  // Log but continue (configurable)
  | "info";    // Silently continue
```

#### 5.2 Add `failureSeverity` to base node config

```typescript
export interface BaseNodeConfig {
  // ... existing fields ...
  /** NEW: Override the default failure severity for this node's sub-executions */
  failureSeverity?: SubExecutionFailureSeverity;
}
```

---

## Implementation Order

| Phase | Priority | Scope | Complexity | Risk |
|---|---|---|---|---|
| **Phase 1: Fork isolation** | P0 | 1 file (`fork-handler.ts`) + types | Medium | Low (backward compatible) |
| **Phase 2: Fork-Join integration** | P0 | Verification only | Low | None |
| **Phase 3: Subgraph strategy** | P1 | 1 file (`subgraph-handler.ts`) + types | Medium | Low (backward compatible) |
| **Phase 4: Loop strategy** | P1 | 2 files + types + coordinator | High | Medium |
| **Phase 5: Unified classification** | P2 | 1 file + types | Low | Low (new feature) |

## Files to Modify

| File | Phase | Change Summary |
|---|---|---|
| `packages/sdk/workflow/execution/handlers/node-handlers/fork-handler.ts` | P1 | Replace `Promise.all` with `Promise.allSettled`, add failure strategy logic |
| `packages/sdk/workflow/execution/types/fork.types.ts` | P1 | Add `failureStrategy` and `maxFailedBranches` to `ForkExecutionConfig` |
| `packages/sdk/types/...` (ForkNodeConfig) | P1 | Add `failureStrategy` and `maxFailedBranches` to `ForkNodeConfig` |
| `packages/sdk/workflow/execution/handlers/node-handlers/subgraph-handler.ts` | P3 | Add `onFailure`/`maxRetries`/`retryDelayMs`/`fallbackOutput` handling |
| `packages/sdk/types/...` (SubgraphNodeConfig) | P3 | Add `onFailure`/`maxRetries`/`retryDelayMs`/`fallbackOutput` fields |
| `packages/sdk/workflow/execution/handlers/node-handlers/loop-start-handler.ts` | P4 | Add `onIterationFailure`/`maxConsecutiveFailures` support |
| `packages/sdk/workflow/execution/handlers/node-handlers/loop-end-handler.ts` | P4 | Add iteration failure state tracking |
| `packages/sdk/workflow/execution/coordinators/node-execution-coordinator.ts` | P4 | Add loop-aware failure handling |
| `packages/sdk/workflow/execution/coordinators/workflow-execution-coordinator.ts` | P4 | Add loop-aware failure routing |
| `packages/sdk/shared/types/execution.ts` | P5 | Add `SubExecutionFailureSeverity` type |
| `packages/sdk/types/...` (BaseNodeConfig) | P5 | Add `failureSeverity` field |

## Backward Compatibility

All proposed changes are backward compatible:

- **Phase 1**: `Promise.allSettled` with explicit failure handling produces the same behavior as `Promise.all` when all branches succeed. The new `failureStrategy` defaults to `"fail-fast"` which matches current behavior.
- **Phase 3**: The `onFailure` field defaults to `"fail"`, matching current behavior.
- **Phase 4**: The `onIterationFailure` defaults to `"fail"`, matching current behavior.
- **Phase 5**: The `failureSeverity` is optional; when absent, the existing behavior applies.

## Verification Plan

| Phase | Verification |
|---|---|
| P1 | Unit test: FORK with 3 branches where 1 fails → other 2 should complete and be reported |
| P1 | Unit test: FORK with `continue-on-error` → all branches execute, Join receives all statuses |
| P1 | Integration test: FORK+JOIN with `ALL_COMPLETED` strategy → fails correctly when branches fail |
| P1 | Integration test: FORK+JOIN with `ANY_COMPLETED` strategy → succeeds when at least one branch completes |
| P3 | Unit test: SUBGRAPH with `onFailure=continue` → parent workflow continues on subgraph failure |
| P3 | Unit test: SUBGRAPH with `onFailure=retry` → retries N times before failing |
| P4 | Integration test: LOOP with `onIterationFailure=skip` → skips failed iterations |
| P5 | Type test: New types are exported and usable |
| Overall | All existing tests must pass with no regressions |