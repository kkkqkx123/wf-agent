# Shared Protection Mechanisms

## 1. Overview

The protection mechanisms provide timeout management and tool failure protection for execution entities. They prevent runaway executions and handle tool call failures gracefully.

## 2. TimeoutManager

Manages per-node and wall-clock timeouts for execution entities:

```
TimeoutManager
├── Configuration
│   ├── wallClockTimeout: number?  // Total execution timeout
│   ├── defaultNodeTimeout: number?  // Default per-node timeout
│   └── perNodeTimeouts: Map<ID, number>  // Per-node overrides
│
├── Timeout Registration
│   ├── setTimeout(nodeId, timeoutMs) → TimeoutHandle
│   │   ├── Register timeout for node
│   │   ├── Returns handle for cancellation
│   │   └── Emits timeout event on expiry
│   │
│   ├── clearTimeout(handle) → void
│   │   └── Cancel registered timeout
│   │
│   └── clearAllTimeouts() → void
│       └── Cancel all registered timeouts
│
├── Wall Clock Timer
│   ├── startWallClockTimer() → void
│   ├── getElapsedWallClockTime() → number
│   ├── getRemainingWallClockTime() → number
│   └── isWallClockExpired() → boolean
│
├── Query
│   ├── getActiveTimeouts() → TimeoutHandle[]
│   ├── getRemainingTime(nodeId) → number?
│   └── isTimeoutExpired(nodeId) → boolean
│
└── Checkpoint Support
    ├── createSnapshot() → TimeoutManagerSnapshot
    └── restoreFromSnapshot(snapshot) → void
```

### TimeoutHandle

```typescript
interface TimeoutHandle {
  id: string;
  nodeId?: ID;
  timeoutMs: number;
  startTime: number;
  endTime: number;
  callback: () => void;
  isExpired: boolean;
  clear(): void;
}
```

### Timeout Events

| Event | Description |
|-------|-------------|
| `NODE_TIMEOUT` | Per-node execution timeout |
| `WALL_CLOCK_TIMEOUT` | Total execution timeout |
| `TOOL_TIMEOUT` | Tool call execution timeout |

## 3. ToolFailureProtectionState

Manages tool failure protection state to prevent cascading failures:

```
ToolFailureProtectionState
├── Configuration
│   ├── maxConsecutiveFailures: number  // Default: 3
│   ├── maxTotalFailures: number  // Default: 10
│   ├── cooldownPeriod: number  // Ms to wait after too many failures
│   └── perToolLimits: Map<string, ToolFailureLimit>
│
├── Tracking
│   ├── consecutiveFailures: number
│   ├── totalFailures: number
│   ├── lastFailureTime: number?
│   ├── perToolFailures: Map<string, ToolFailureCount>
│   └── cooldownUntil: number?
│
├── State Query
│   ├── isProtectionTriggered() → boolean
│   ├── getFailureCount() → TotalFailureCount
│   ├── getToolFailureCount(toolName) → number
│   ├── getConsecutiveFailureCount() → number
│   ├── isInCooldown() → boolean
│   └── getRemainingCooldown() → number
│
├── Failure Recording
│   ├── recordFailure(toolName) → void
│   │   ├── Increment consecutive and total counters
│   │   ├── Increment per-tool counter
│   │   ├── Check if protection should trigger
│   │   └── Set cooldown if triggered
│   │
│   ├── recordSuccess(toolName) → void
│   │   └── Reset consecutive failure counter
│   │
│   └── reset() → void
│       └── Reset all counters
│
├── Protection Actions
│   ├── getProtectionAction(toolName) → ProtectionAction
│   │   ├── NONE: No protection needed
│   │   ├── COOLDOWN: Wait before retrying
│   │   ├── SKIP_TOOL: Skip this tool for current execution
│   │   └── STOP_EXECUTION: Stop the entire execution
│   │
│   └── getSuggestedAction() → ProtectionAction
│       └── Get current suggested action
│
└── Checkpoint Support
    ├── createSnapshot() → ToolFailureProtectionSnapshot
    └── restoreFromSnapshot(snapshot) → void
```

### ToolFailureLimit

```typescript
interface ToolFailureLimit {
  maxConsecutiveFailures: number;
  maxTotalFailures: number;
  cooldownPeriod: number;
}
```

### ProtectionAction

```typescript
enum ProtectionAction {
  NONE = "none",
  COOLDOWN = "cooldown",
  SKIP_TOOL = "skip_tool",
  STOP_EXECUTION = "stop_execution",
}
```

### TotalFailureCount

```typescript
interface TotalFailureCount {
  consecutiveFailures: number;
  totalFailures: number;
  perToolFailures: Map<string, number>;
}
```

## 4. Protection Flow

### Tool Failure Protection

```
Tool execution fails:
  1. ToolFailureProtectionState.recordFailure(toolName)
  2. Check protection thresholds:
     a. If consecutive failures > max → trigger cooldown
     b. If total failures > max → suggest STOP_EXECUTION
     c. If per-tool failures > limit → suggest SKIP_TOOL
  3. Return ProtectionAction
  4. Execution engine handles the action:
     - NONE: continue normally
     - COOLDOWN: wait before retry
     - SKIP_TOOL: skip tool for this execution
     - STOP_EXECUTION: fail the execution
```

## 5. Integration

Both TimeoutManager and ToolFailureProtectionState are integrated into the execution entity:

```
AgentLoopEntity / WorkflowExecutionEntity
├── timeoutManager: TimeoutManager
│   ├── Manages per-node and wall-clock timeouts
│   └── Emits timeout events on expiry
│
└── toolFailureProtectionState: ToolFailureProtectionState
    ├── Tracks tool call failures
    └── Suggests protection actions
```