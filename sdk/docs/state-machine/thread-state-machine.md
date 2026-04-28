# Thread State Machine

The Thread state machine manages the execution lifecycle of workflow thread instances. It follows a state pattern where threads transition from creation through running to terminal states.

## Status Definition

### ThreadStatus Enum

| Status | Description | Type |
|--------|-------------|------|
| `CREATED` | Thread created, not started | Initial |
| `RUNNING` | Thread is executing | Active |
| `PAUSED` | Thread is paused (can be resumed) | Active |
| `COMPLETED` | Thread completed successfully | Terminal (Success) |
| `FAILED` | Thread execution failed | Terminal (Error) |
| `CANCELLED` | Thread was cancelled | Terminal (Error) |
| `TIMEOUT` | Thread execution timed out | Terminal (Error) |

**Source**: `packages/types/src/thread/status.ts`

## Status Transition Rules

```
CREATED  → RUNNING
RUNNING  → PAUSED | COMPLETED | FAILED | CANCELLED | TIMEOUT
PAUSED  → RUNNING | CANCELLED | TIMEOUT
COMPLETED/FAILED/CANCELLED/TIMEOUT → (final states, no further transitions)
```

### Transition Table

| From \ To | CREATED | RUNNING | PAUSED | COMPLETED | FAILED | CANCELLED | TIMEOUT |
|-----------|--------|---------|-------|----------|--------|----------|---------|
| CREATED   | -      | ✓       | -     | -        | -      | -         | -       |
| RUNNING   | -      | -       | ✓     | ✓       | ✓      | ✓        | ✓       |
| PAUSED   | -      | ✓       | -     | -        | -      | ✓        | ✓       |
| COMPLETED| -      | -       | -     | -        | -      | -         | -       |
| FAILED   | -      | -       | -     | -        | -      | -         | -       |
| CANCELLED| -      | -       | -     | -        | -      | -         | -       |
| TIMEOUT  | -      | -       | -     | -        | -      | -         | -       |

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

### 1. ThreadState (State Manager)

**Location**: `sdk/graph/state-managers/thread-state.ts`

Manages runtime state during thread execution:
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

### 2. ThreadStateTransitor (State Transition Handler)

**Location**: `sdk/graph/execution/coordinators/thread-state-transitor.ts`

Provides atomic state transition operations:

| Method | Valid From | Operation |
|--------|------------|-----------|
| `startThread()` | CREATED | Start execution |
| `pauseThread()` | RUNNING | Pause execution |
| `resumeThread()` | PAUSED | Resume execution |
| `completeThread()` | RUNNING | Complete with result |
| `failThread()` | RUNNING | Mark as failed |
| `cancelThread()` | RUNNING/PAUSED | Cancel execution |

The transitor also:
- Validates state transitions
- Emits lifecycle events
- Manages child thread cascade operations
- Coordinates with conversation session cleanup

### 3. ThreadStateValidator (Validation Rules)

**Location**: `sdk/graph/execution/utils/thread-state-validator.ts`

Pure functions for state validation:

| Function | Description |
|----------|-------------|
| `isValidTransition(from, to)` | Check if transition is valid |
| `validateTransition(id, from, to)` | Validate or throw error |
| `getAllowedTransitions(from)` | Get allowed target states |
| `isTerminalStatus(status)` | Check if terminal state |
| `isActiveStatus(status)` | Check if active state |

## Thread Entity Integration

**Location**: `sdk/graph/entities/thread-entity.ts`

The `ThreadEntity` encapsulates both persistent data and runtime state:

```typescript
class ThreadEntity {
  private _status: ThreadStatus = "CREATED";
  private state: ThreadState;

  getStatus(): ThreadStatus;
  setStatus(status: ThreadStatus): void;
}
```

## Event Integration

State transitions emit lifecycle events:

| Event | Triggered By |
|-------|------------|
| `ThreadStarted` | `startThread()` |
| `ThreadPaused` | `pauseThread()` |
| `ThreadResumed` | `resumeThread()` |
| `ThreadCompleted` | `completeThread()` |
| `ThreadFailed` | `failThread()` |
| `ThreadCancelled` | `cancelThread()` |
| `ThreadStateChanged` | Any state transition |

**Source**: `sdk/core/utils/event/builders/thread-events.ts`

## Usage in Execution Flow

1. **Thread Creation**: Status starts as `CREATED`
2. **Thread Start**: `startThread()` -> transitions to `RUNNING`
3. **During Execution**: Various transitions based on execution flow
4. **Thread Completion**: Final transition to terminal state

## Notes

- Thread status is persisted in the `Thread` entity storage
- Runtime state (`ThreadState`) is ephemeral and used during execution
- Valid transitions are enforced to prevent invalid state changes
- Child threads can be cascade-cancelled when parent is cancelled
- State transitions support interruption (pause/stop flags)