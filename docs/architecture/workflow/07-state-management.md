# State Management

## 1. State Manager Architecture

Each state manager implements the `StateManager<T>` interface, providing unified snapshot/restore for checkpoint:

```typescript
interface StateManager<T> {
  createSnapshot(): T;
  restoreFromSnapshot(snapshot: T): void;
  cleanup(): void;
  reset(): void;
  size(): number;
  isEmpty(): boolean;
}
```

## 2. WorkflowExecutionState

Manages the execution status, error records, and interruption history.

### Status Transitions

```
PENDING → RUNNING → COMPLETED
                  → FAILED
                  → PAUSED → RUNNING → COMPLETED
                                      → FAILED
                                      → CANCELLED
```

### Error Management

- **Error Chain**: Records parent-child error relationships via `ErrorChainManager`
- **Root Cause Analysis**: `getRootCauseError()` traces back to the originating error
- **Error Pattern Analysis**: `analyzeErrorPattern()` identifies recurring error patterns
- **Recovery Recommendations**: `getRecommendedRecoveryAction()` suggests retry/fallback/manual intervention

### Interruption Records

Tracks pause/stop/resume events with:
- Operation type and timestamp
- Node ID where interruption occurred
- Reason and metadata
- Resume context for restoration

## 3. ExecutionState

Manages the subgraph execution stack:

```
Subgraph Stack:
  [0] root workflow
  [1] subgraph A (depth 1)
  [2] subgraph A.1 (depth 2)
  ...
```

- `enterSubgraph(workflowId, parentWorkflowId, input)` → Push context
- `exitSubgraph()` → Pop context
- `getCurrentSubgraphContext()` → Current subgraph info
- `getCurrentWorkflowId(baseWorkflowId)` → Resolve active workflow ID

## 4. ForkJoinState

Manages fork/join execution context:

- `setForkId(forkId)` / `getForkId()` → Track which FORK spawned this branch
- `setForkPathId(forkPathId)` / `getForkPathId()` → Track branch path
- `isForkBranch()` → Check if this is a fork branch execution
- `getAggregationState()` / `setAggregationState()` → JOIN node result aggregation

## 5. VariableManager

Manages scoped variables with a priority-based resolution:

```
Variable Resolution Priority (highest → lowest):
  1. Loop scope (innermost loop first)
  2. Subgraph scope
  3. Execution scope
  4. Global scope
```

- `initializeFromDefinitions(definitions)` → Initialize from VariableDefinition[]
- `getVariable(name)` → Resolve by scope priority
- `setVariable(name, value)` → Set at execution scope
- `copyFrom(source)` → Copy variables (for fork branch isolation)
- Scope management: pushScope/popScope for loops and subgraphs

## 6. WorkflowStateCoordinator

Manages the `ConversationSession` (message history) state:

- Extends `BaseStateCoordinator` for shared message management
- Parent-child message passing: `exportMessagesForChild()`, `importMessagesFromChild()`
- Single data source: Messages only stored in `ConversationSession` (eliminated dual-write)

## 7. InterruptionState

Manages pause/stop signals via `AbortController`:

- `pause()` → Set pause flag, abort signal
- `stop()` → Set stop flag, abort signal
- `reset()` → Clear interruption flags
- `getAbortSignal()` → Get signal for async operations
- Used by `InterruptionDetector` to check execution status