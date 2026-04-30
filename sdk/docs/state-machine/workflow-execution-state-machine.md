# Workflow Execution State Machine

The Workflow Execution state machine manages the execution lifecycle of workflow execution instances. It follows a state pattern where executions transition from creation through running to terminal states.

## Status Definition

### WorkflowExecutionStatus Enum

| Status | Description | Type |
|--------|-------------|------|
| `CREATED` | Execution created, not started | Initial |
| `RUNNING` | Execution is running | Active |
| `PAUSED` | Execution is paused (can be resumed) | Active |
| `COMPLETED` | Execution completed successfully | Terminal (Success) |
| `FAILED` | Execution failed | Terminal (Error) |
| `CANCELLED` | Execution was cancelled | Terminal (Error) |
| `TIMEOUT` | Execution timed out | Terminal (Error) |

**Source**: `packages/types/src/workflow/status.ts`

## Status Transition Rules

```
CREATED  → RUNNING
RUNNING  → PAUSED | COMPLETED | FAILED | CANCELLED | TIMEOUT
PAUSED   → RUNNING | CANCELLED | TIMEOUT
COMPLETED/FAILED/CANCELLED/TIMEOUT → (final states, no further transitions)
```

### Transition Table

| From \ To | CREATED | RUNNING | PAUSED | COMPLETED | FAILED | CANCELLED | TIMEOUT |
|-----------|--------|---------|-------|----------|--------|----------|---------|
| CREATED   | -      | ✓       | -     | -        | -      | -         | -       |
| RUNNING   | -      | -       | ✓     | ✓       | ✓      | ✓        | ✓       |
| PAUSED    | -      | ✓       | -     | -        | -      | ✓        | ✓       |
| COMPLETED | -      | -       | -     | -        | -      | -         | -       |
| FAILED    | -      | -       | -     | -        | -      | -         | -       |
| CANCELLED | -      | -       | -     | -        | -      | -         | -       |
| TIMEOUT   | -      | -       | -     | -        | -      | -         | -       |

## State Classification

### Terminal States
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`

### Active States
- `RUNNING`
- `PAUSED`

## Implementation Components

### 1. WorkflowExecutionState (State Manager)

**Location**: `sdk/workflow/state-managers/workflow-execution-state.ts`

Manages runtime state during execution:
- `_status`: Current status
- `_shouldPause`: Pause interrupt flag
- `_shouldStop`: Stop interrupt flag
- `_startTime`: Execution start timestamp
- `_endTime`: Execution end timestamp
- `_error`: Error message

**Key Methods**:
| Method | Description |
|--------|-------------|
| `start()` | Transition to RUNNING |
| `pause()` | Transition to PAUSED |
| `resume()` | Transition to RUNNING |
| `complete()` | Transition to COMPLETED |
| `fail(error)` | Transition to FAILED |
| `cancel()` | Transition to CANCELLED |
| `timeout()` | Transition to TIMEOUT |
| `interrupt(type)` | Set interrupt flag |
| `isRunning()` | Check if RUNNING |
| `isPaused()` | Check if PAUSED |
| `isTerminal()` | Check if terminal |

### 2. WorkflowStateTransitor (State Transition Handler)

**Location**: `sdk/workflow/execution/coordinators/workflow-state-transitor.ts`

Provides atomic state transition operations:

| Method | Valid From | Operation |
|--------|------------|-----------|
| `startWorkflowExecution()` | CREATED | Start execution |
| `pauseWorkflowExecution()` | RUNNING | Pause execution |
| `resumeWorkflowExecution()` | PAUSED | Resume execution |
| `completeWorkflowExecution()` | RUNNING | Complete with result |
| `failWorkflowExecution()` | RUNNING | Mark as failed |
| `cancelWorkflowExecution()` | RUNNING/PAUSED | Cancel execution |

The transitor also:
- Validates state transitions
- Emits lifecycle events
- Manages child execution cascade operations
- Coordinates with conversation session cleanup

### 3. State Validation (Validation Rules)

Pure functions for state validation:

| Function | Description |
|----------|-------------|
| `isValidTransition(from, to)` | Check if transition is valid |
| `validateTransition(id, from, to)` | Validate or throw error |
| `getAllowedTransitions(from)` | Get allowed target states |
| `isTerminalStatus(status)` | Check if terminal state |
| `isActiveStatus(status)` | Check if active state |

## Workflow Execution Entity Integration

**Location**: `sdk/workflow/entities/workflow-execution-entity.ts`

The `WorkflowExecutionEntity` encapsulates both persistent data and runtime state:

```typescript
class WorkflowExecutionEntity {
  private _status: WorkflowExecutionStatus = "CREATED";
  private state: WorkflowExecutionState;

  getStatus(): WorkflowExecutionStatus;
  setStatus(status: WorkflowExecutionStatus): void;
}
```

## Event Integration

State transitions emit lifecycle events:

| Event | Triggered By |
|-------|------------|
| `WorkflowExecutionStarted` | `startWorkflowExecution()` |
| `WorkflowExecutionPaused` | `pauseWorkflowExecution()` |
| `WorkflowExecutionResumed` | `resumeWorkflowExecution()` |
| `WorkflowExecutionCompleted` | `completeWorkflowExecution()` |
| `WorkflowExecutionFailed` | `failWorkflowExecution()` |
| `WorkflowExecutionCancelled` | `cancelWorkflowExecution()` |
| `WorkflowExecutionStateChanged` | Any state transition |

**Source**: `sdk/core/utils/event/builders/workflow-execution-events.ts`

## Usage in Execution Flow

1. **Execution Creation**: Status starts as `CREATED`
2. **Execution Start**: `startWorkflowExecution()` -> transitions to `RUNNING`
3. **During Execution**: Various transitions based on execution flow
4. **Execution Completion**: Final transition to terminal state

## Notes

- Execution status is persisted in the `WorkflowExecution` entity storage
- Runtime state (`WorkflowExecutionState`) is ephemeral and used during execution
- Valid transitions are enforced to prevent invalid state changes
- Child executions can be cascade-cancelled when parent is cancelled
- State transitions support interruption (pause/stop flags)
