# Agent Interruption and Error Handling

## 1. Interruption System

The agent interruption system provides a unified mechanism to pause/stop agent loop executions gracefully.

### Architecture

```
AgentExecutionInterruptedException (extends InterruptedException)
├── message: string
├── interruptionType: InterruptionType (PAUSE | STOP)
├── agentLoopId?: string
├── iteration?: number
└── context?: Record<string, unknown>
```

The interruption system integrates with the shared `InterruptionState`:

```
InterruptionState (per execution entity)
├── AbortController (primary interrupt mechanism)
├── pause() → set flag + abort
├── stop() → set flag + abort
├── reset() → clear flags
├── isPaused() → boolean
├── isStopped() → boolean
└── getAbortSignal() → AbortSignal
```

### Interruption Handling Flow

```
executeWithInterruptionHandling(callback):
  1. Wrap callback with AbortSignal check
  2. On abort signal:
     a. Check interruption type (PAUSE vs STOP)
     b. For PAUSE: Save state, create checkpoint, return pause result
     c. For STOP: Clean up resources, return cancel result
```

### Interruption Detection

The `checkAgentInterruption()` utility checks for interruptions at each iteration boundary:

```
checkAgentInterruption(entity):
  1. Check InterruptionState flags
  2. Check AbortController signal
  3. If paused: throw AgentExecutionInterruptedException (PAUSE)
  4. If stopped: throw AgentExecutionInterruptedException (STOP)
  5. Return interruption description if interrupted
```

### Interruption Patterns

- **Graceful Pause**: Current iteration completes, then agent loop pauses
- **Immediate Stop**: Agent loop stops mid-iteration
- **Resume**: Agent loop restored from checkpoint
- **Skip completed iterations**: On resume, completed iterations are already in history

## 2. Error Handling

### Error Types

| Error Type | Description |
|-----------|-------------|
| `AgentExecutionInterruptedException` | Interruption during execution (pause/stop) |
| `SDKError` | Standardized SDK-level errors |
| `RuntimeValidationError` | Validation failures during execution |
| Tool execution errors | Errors from tool calls |

### Error Handler Architecture

The `handleAgentError()` function provides unified error handling:

```
handleAgentError(entity, error, eventManager, context):
  1. Standardize error to SDKError
  2. Build error context (agentLoopId, iteration, toolCallCount)
  3. Determine severity:
     - ERROR: Stop execution
     - WARNING: Allow continuation
     - INFO: Log and continue
  4. Record error in state (error chain)
  5. Emit error event
  6. Log error with context
  7. Return error handler result
```

### Error Context Building

```typescript
function buildAgentErrorContext(entity, operation, additionalContext): ErrorContext {
  return {
    executionId: entity.id,
    nodeId: entity.nodeId,
    operation,
    iteration: entity.state.currentIteration,
    toolCallCount: entity.state.toolCallCount,
    ...additionalContext,
  };
}
```

### Error Standardization

```typescript
function standardizeAgentError(error, context): SDKError {
  // If already SDKError, return directly
  // Otherwise wrap as SDKError with ERROR level
}
```

## 3. Retry with Exponential Backoff

The agent loop supports retry with exponential backoff for recoverable errors.

### Retry Configuration

```typescript
interface RetryPolicy {
  enabled: boolean;
  maxRetries?: number;       // Max retry attempts
  baseDelayMs?: number;      // Base delay in ms (default: 1000)
  maxDelayMs?: number;       // Max delay in ms (default: 30000)
  backoffMultiplier?: number; // Backoff multiplier (default: 2)
}
```

### Retry Execution Flow

```
executeIterationWithRetryAndTimeout(entity, executor, retryPolicy, timeoutMs, retryBudget):
  1. Calculate maxAttempts from retryPolicy
  2. For each attempt:
     a. Check retry budget before attempting
     b. Execute iteration with timeout
     c. On success: return result
     d. On error:
        - Check if recoverable (isRecoverableAgentError)
        - If recoverable and attempts remain:
          * Calculate backoff delay
          * Check retry budget for delay
          * Wait for backoff
          * Retry
        - If not recoverable or no attempts left: throw
```

### Retry Budget

The `RetryBudget` tracks retry delays and execution time:

```
RetryBudget
├── isExhausted() → boolean
├── recordRetryDelay(delay) → void
├── recordExecutionTime(time) → void
├── getTotalRetryDelay() → number
├── getTotalExecutionTime() → number
└── reset() → void
```

**Time Budget Modes**:
- `delay-only`: Only retry delays count toward budget
- `total-time`: Both retry delays and execution time count

## 4. Error Recovery

### Continue with Error Context

The agent loop supports continuing with error context injection:

```
continueAgentLoop(agentLoopId, options):
  1. Load entity from registry
  2. Create error context message
  3. Inject error context into conversation
  4. Resume execution
  5. LLM receives error context for informed recovery
```

### Error Recovery Flow

```
Error → isRecoverable?
  ├── Yes, retries left → exponential backoff → retry
  ├── Yes, no retries → continue with error context
  └── No → fail agent loop
```

## 5. Timeout Management

### Per-Execution Timeout

The agent loop supports a configurable total execution timeout:

```
AgentLoopExecuteOptions:
  ├── timeoutMs?: number  // Total execution timeout
  └── ...
```

When the timeout is reached, the execution is interrupted with a `PAUSE` signal.

### Per-Tool Call Timeout

Individual tool calls can have their own timeout (configured via `ToolExecutionCoordinator`):

```
ToolExecutionCoordinatorDependencies:
  ├── toolTimeout?: number  // Per-tool execution timeout
  └── ...
```

## 6. State Cleanup on Error

When an error occurs and the agent loop is terminated:

```
handleAgentError(entity):
  1. Record error in AgentLoopState
  2. Update entity status to FAILED
  3. Emit AGENT_FAILED event
  4. Clean up resources (timeouts, pending tool calls)
  5. Create checkpoint if configured (ON_ERROR trigger)
  6. Return AgentLoopResult with error
```