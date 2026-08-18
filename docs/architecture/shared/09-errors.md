# Shared Error Management

## 1. Overview

The shared error management system provides unified error handling, error chain management, and error utilities used by both workflow and agent modules.

## 2. ErrorChainManager

Manages a chain of errors for tracking parent-child error relationships:

```
ErrorChainManager
├── Error Recording
│   ├── recordError(errorRecord) → string
│   │   ├── Generate error ID
│   │   ├── Link to parent error (if in chain)
│   │   ├── Store error record
│   │   └── Return error ID
│   │
│   ├── recordErrorWithChain(error, parentErrorId?) → string
│   │   └── Record error with explicit parent link
│   │
│   └── clearErrors() → void
│
├── Chain Traversal
│   ├── getErrorChain(fromErrorId) → ExecutionErrorRecord[]
│   │   └── Traverse from error to root cause
│   │
│   ├── getRootCauseError() → ExecutionErrorRecord | null
│   │   └── Find the originating error
│   │
│   ├── getErrorHistory() → ExecutionErrorRecord[]
│   │   └── Get all errors in chronological order
│   │
│   └── getErrorCount() → number
│
├── Analysis
│   ├── analyzeErrorPattern() → ErrorPattern
│   │   ├── Identify recurring error patterns
│   │   ├── Calculate error depth
│   │   └── Return pattern analysis
│   │
│   ├── getRecommendedRecoveryAction() → RecoveryAction
│   │   └── Suggest retry/fallback/manual intervention
│   │
│   └── getErrorSummary() → ErrorSummary
│       └── Aggregate error statistics
│
└── Checkpoint Support
    ├── createSnapshot() → ErrorChainSnapshot
    └── restoreFromSnapshot(snapshot) → void
```

### ExecutionErrorRecord

```typescript
interface ExecutionErrorRecord {
  id: string;
  parentId?: string;
  message: string;
  code?: string;
  severity: "info" | "warning" | "error" | "fatal";
  timestamp: number;
  context?: ErrorContext;
  stack?: string;
  metadata?: Record<string, unknown>;
}
```

### ErrorContext

```typescript
interface ErrorContext {
  executionId?: string;
  nodeId?: string;
  operation?: string;
  iteration?: number;
  toolCallCount?: number;
  [key: string]: unknown;
}
```

### ErrorPattern

```typescript
interface ErrorPattern {
  type: "none" | "single" | "chain" | "recurring";
  depth: number;
  rootCause?: ExecutionErrorRecord;
  patterns: string[];
  frequency: number;
  recommendation?: string;
}
```

## 3. Error Utilities

### handleErrorWithContext

Unified error handler with context injection:

```typescript
function handleErrorWithContext(
  error: Error,
  context: ErrorContext,
  options?: ErrorHandlerOptions
): SDKError
```

### emitErrorEvent

Emits error events to the event registry:

```typescript
function emitErrorEvent(
  eventManager: EventRegistry,
  error: SDKError,
  context: ErrorContext
): Promise<void>
```

### logError

Logs error with context to the logger:

```typescript
function logError(
  error: SDKError,
  context: ErrorContext,
  logger: Logger
): void
```

## 4. Error Types

### SDKError

The standard SDK error type:

```typescript
class SDKError extends Error {
  constructor(
    message: string,
    public readonly level: "info" | "warning" | "error" | "fatal",
    public readonly context?: ErrorContext,
    public readonly cause?: Error,
  ) {
    super(message);
    this.name = "SDKError";
  }

  isRecoverable(): boolean;
  getSeverityLevel(): string;
  toJSON(): Record<string, unknown>;
}
```

### Common Error Classes

| Error Class | Description | Severity |
|-------------|-------------|----------|
| `SDKError` | Standard SDK error | Configurable |
| `ValidationError` | Configuration/input validation error | error |
| `ConfigurationValidationError` | Schema validation error | error |
| `RuntimeValidationError` | Runtime validation error | error |
| `StateManagementError` | State transition error | error |
| `InterruptedException` | Execution interruption | warning |
| `AgentCheckpointError` | Checkpoint operation error | error |

## 5. Error Recovery

### Recovery Actions

```typescript
enum RecoveryAction {
  RETRY = "retry",
  FALLBACK = "fallback",
  CONTINUE = "continue",
  MANUAL_INTERVENTION = "manual_intervention",
  ABORT = "abort",
}
```

### Recovery Flow

```
Error occurs → ErrorHandler.handleError()
  ├── Record error in ErrorChainManager
  ├── Analyze error pattern
  ├── Determine recovery action:
  │   ├── RETRY: retry the operation
  │   ├── FALLBACK: execute fallback strategy
  │   ├── CONTINUE: skip and continue
  │   ├── MANUAL_INTERVENTION: report to user
  │   └── ABORT: stop execution
  └── Execute recovery action
```

## 6. Error Severity Levels

| Level | Description | Behavior |
|-------|-------------|----------|
| `info` | Informational, no action needed | Log and continue |
| `warning` | Non-critical issue | Log, warn, continue |
| `error` | Execution error | Stop current operation |
| `fatal` | System error | Stop entire execution |

## 7. Integration

The error management system is integrated into both workflow and agent execution:

```
WorkflowExecutionEntity / AgentLoopEntity
├── errorChainManager: ErrorChainManager
│   └── Tracks error chain for root cause analysis
│
└── Error handling via error-utils
    ├── handleErrorWithContext(error, context)
    ├── emitErrorEvent(eventManager, error, context)
    └── logError(error, context, logger)
```