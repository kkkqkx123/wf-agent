# Shared Interruption System

## 1. Overview

The interruption system provides a unified mechanism to pause/stop execution entities gracefully across both workflow and agent modules. It uses a combination of AbortController signals and state flags.

## 2. InterruptionState

The per-execution entity interruption state manager:

```
InterruptionState
├── Control Flags
│   ├── _shouldPause: boolean
│   ├── _shouldStop: boolean
│   ├── pause() → void
│   ├── stop() → void
│   ├── resume() → void
│   └── reset() → void
│
├── Query
│   ├── isPaused() → boolean
│   ├── isStopped() → boolean
│   ├── isInterrupted() → boolean
│   └── getInterruptionType() → InterruptionType | null
│
├── AbortController Integration
│   ├── abortController: AbortController
│   ├── getAbortSignal() → AbortSignal
│   ├── abort() → void
│   └── resetAbortController() → void
│
└── Checkpoint Support
    ├── createSnapshot() → InterruptionStateSnapshot
    └── restoreFromSnapshot(snapshot) → void
```

### InterruptionType

```typescript
enum InterruptionType {
  PAUSE = "pause",
  STOP = "stop",
  TIMEOUT = "timeout",
}
```

## 3. Interruption Utilities

### executeWithInterruptionHandling

Wraps an async operation with interruption detection:

```typescript
async function executeWithInterruptionHandling<T>(
  callback: (signal: AbortSignal) => Promise<T>,
  interruptionState: InterruptionState,
): Promise<InterruptionResult<T>>
```

Flow:
```
executeWithInterruptionHandling(callback, interruptionState):
  1. Get AbortSignal from interruptionState
  2. Execute callback with signal
  3. On abort signal:
     a. Determine interruption type (PAUSE vs STOP vs TIMEOUT)
     b. For PAUSE: return pause result
     c. For STOP: return stop result
     d. For TIMEOUT: return timeout result
  4. On success: return result
```

### iterateWithInterruptionHandling

Wraps an iteration loop with interruption detection:

```typescript
async function iterateWithInterruptionHandling<T>(
  iterator: () => Promise<T>,
  interruptionState: InterruptionState,
  options?: { onPause?: () => Promise<void>; onStop?: () => Promise<void> },
): Promise<InterruptionResult<T>>
```

### checkInterruption

Checks if an execution has been interrupted:

```typescript
function checkInterruption(interruptionState: InterruptionState): InterruptionCheckResult

interface InterruptionCheckResult {
  interrupted: boolean;
  type?: InterruptionType;
  reason?: string;
}
```

### combineAbortSignals

Combines multiple AbortSignals into one:

```typescript
function combineAbortSignals(signals: AbortSignal[]): AbortSignal
```

## 4. InterruptedException

The base exception class for interruptions:

```typescript
class InterruptedException extends Error {
  constructor(
    message: string,
    public readonly interruptionType: InterruptionType,
    public readonly context?: Record<string, unknown>,
  ) {
    super(message);
    this.name = "InterruptedException";
  }

  isPause(): boolean;
  isStop(): boolean;
  isTimeout(): boolean;
}
```

### Agent-Specific Extension

```typescript
class AgentExecutionInterruptedException extends InterruptedException {
  constructor(
    message: string,
    interruptionType: InterruptionType,
    public readonly agentLoopId?: string,
    public readonly iteration?: number,
    context?: Record<string, unknown>,
  ) {
    super(message, interruptionType, context);
  }
}
```

## 5. Interruption Flow

### Pause Flow

```
pause(entity):
  1. Set interruptionState.pause()
  2. Abort AbortController
  3. Current operation detects abort signal
  4. Graceful pause:
     a. Complete current operation (iteration, node)
     b. Save state
     c. Create checkpoint (if configured)
     d. Return pause result
  5. Entity status → PAUSED
```

### Resume Flow

```
resume(entity):
  1. Load entity from checkpoint
  2. Reset interruption state
  3. Recreate runtime managers
  4. Entity status → RUNNING
  5. Continue execution from saved state
```

### Stop Flow

```
stop(entity):
  1. Set interruptionState.stop()
  2. Abort AbortController
  3. Current operation detects abort signal
  4. Immediate stop:
     a. Clean up resources
     b. Cancel pending operations
     c. Return cancel result
  5. Entity status → CANCELLED
```

## 6. Interruption Patterns

| Pattern | Description | Use Case |
|---------|-------------|----------|
| Graceful Pause | Complete current operation, then pause | User requests pause mid-execution |
| Immediate Stop | Stop immediately, clean up | Error recovery, admin cancel |
| Timeout | Stop after time limit | Execution timeout exceeded |
| Cancellation | Cancel with resource cleanup | User cancels execution |

## 7. Integration with Checkpoint

On pause, a checkpoint is typically created to allow resume:

```
pause → create checkpoint → save state → return pause result
resume → load checkpoint → restore state → continue execution
```