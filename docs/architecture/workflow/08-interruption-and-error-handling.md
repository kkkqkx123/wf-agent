# Interruption and Error Handling

## 1. Interruption System

The interruption system provides a unified mechanism to pause/stop workflow executions gracefully.

### Architecture

```
InterruptionState (per execution entity)
├── AbortController (primary interrupt mechanism)
├── pause() → set flag + abort
├── stop() → set flag + abort
└── reset() → clear flags

InterruptionDetectorImpl (global service)
├── getAbortSignal(executionId) → AbortSignal
├── isAborted(executionId) → boolean
└── getInterruptionType(executionId) → PAUSE | STOP | null
```

### Interruption Handling Flow

```
executeWithInterruptionHandling(callback):
  1. Wrap callback with AbortSignal check
  2. On abort signal:
     a. Check interruption type (PAUSE vs STOP)
     b. For PAUSE: Save state, return pause result, allow resume
     c. For STOP: Clean up resources, return cancel result
```

### Interruption Patterns

- **Graceful Pause**: Node execution completes, then workflow pauses
- **Immediate Stop**: Workflow stops mid-execution
- **Resume**: Workflow execution restored from the currentNodeId
- **Skip completed nodes**: On resume, already-completed nodes are skipped automatically

## 2. Error Handling

### Error Types

| Error Type | Description |
|-----------|-------------|
| `ExecutionError` | General execution failures |
| `RuntimeValidationError` | Validation failures during execution |
| `StateManagementError` | State transition errors |
| `WorkflowExecutionInterruptedException` | Interruption during execution |
| `SDKError` | SDK-level errors |

### Error Chain Management

```
ErrorChainManager
├── recordError(errorRecord) → Add error to chain
├── getErrorChain(fromErrorId) → Traverse error chain
├── getRootCauseError() → Find originating error
└── analyzeErrorPattern() → Identify recurring patterns
```

### Agent Error Handler

The `agent-error-handler.ts` provides workflow-specific error handling:

- `handleAgentError()` → Classify and handle agent errors
- `isRecoverableAgentError()` → Determine if retry is possible
- `createAgentExecutionError()` → Create structured error records

## 3. Timeout Management

### TimeoutManager

A centralized timeout service used by both workflow and agent execution:

```
TimeoutManager
├── register(options) → TimeoutHandle
│   ├── id: unique timeout ID
│   ├── duration: timeout in milliseconds
│   ├── onTimeout: callback
│   ├── tag: classification tag
│   └── interruptionState: optional interruption coordination
│
└── TimeoutHandle
    ├── cancel() → Cancel the timeout
    └── isExpired() → Check if already expired
```

### Timeout Tags

| Tag | Purpose |
|-----|---------|
| `node-execution` | Per-node execution timeout |
| `llm-call` | LLM API call timeout |
| `tool-execution` | Tool call execution timeout |
| `workflow-wall-clock` | Total workflow execution timeout |

## 4. Failure Policy

### Workflow-Level Failure Policy

Configured via `WorkflowExecutionEntity`:

```typescript
interface FailurePolicy {
  onFailure: "retry" | "continue" | "fail";
  maxRetries: number;
  retryDelay: number;
  exponentialBackoff: boolean;
  fallbackOutput?: Record<string, unknown>;
}
```

### Retry Behavior

- **Workflow-level retry**: Resets state and restarts from the beginning
- **Per-node retry**: Configured via `NodeExecutionConfig.maxRetries`
- **Retry Budget**: `RetryBudget` tracks and limits total retry attempts across the workflow
- **Exponential backoff**: Delay increases exponentially between retries