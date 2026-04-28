# State Machine Comparison

This document provides a comparative analysis of Thread and Agent Loop state machines.

## Quick Comparison

| Aspect | Thread | Agent Loop |
|--------|--------|------------|
| **Entity** | ThreadEntity | AgentLoopEntity |
| **Status Type** | ThreadStatus | AgentLoopStatus |
| **Initial State** | CREATED | CREATED |
| **Active States** | RUNNING, PAUSED | RUNNING, PAUSED |
| **Terminal States** | COMPLETED, FAILED, CANCELLED, TIMEOUT | COMPLETED, FAILED, CANCELLED |
| **State Manager** | ThreadState | AgentLoopState |
| **Transitor** | ThreadStateTransitor | (inline in coordinator) |
| **Validator** | ThreadStateValidator | (inline) |
| **Location** | graph/state-managers/ | agent/entities/ |

## Status Mapping

Both state machines can be mapped to a common ExecutionStatus:

| ThreadStatus | AgentLoopStatus | ExecutionStatus |
|--------------|----------------|------------------|
| CREATED | CREATED | PENDING |
| RUNNING | RUNNING | RUNNING |
| PAUSED | PAUSED | PAUSED |
| COMPLETED | COMPLETED | COMPLETED |
| FAILED | FAILED | FAILED |
| CANCELLED | CANCELLED | CANCELLED |
| TIMEOUT | (N/A) | FAILED |

**Note**: Thread has TIMEOUT status that maps to FAILED.

## State Transition Comparison

### Thread Transitions

```
CREATED → RUNNING → [PAUSED | COMPLETED | FAILED | CANCELLED | TIMEOUT]
                     ↑______|_______|
                     PAUSED → [RUNNING | CANCELLED | TIMEOUT]
```

### Agent Loop Transitions

```
CREATED → RUNNING → [PAUSED | COMPLETED | FAILED | CANCELLED]
                     ↑______|_______|
                     PAUSED → [RUNNING | CANCELLED]
```

## Key Differences

### 1. Scope

| Thread | Agent Loop |
|--------|-----------|
| Controls entire workflow execution | Controls single agent iteration loop |
| May have child threads | May be part of a thread |

### 2. Iteration Model

| Thread | Agent Loop |
|--------|-----------|
| Single execution phase | Multiple iterations |
| No iteration tracking | Tracks iteration count/history |
| No tool call tracking | Tracks tool calls per iteration |

### 3. Timeout Handling

| Thread | Agent Loop |
|--------|-----------|
| Has TIMEOUT status | No timeout status |
| Timeout via config | Timeout via max iterations |

### 4. Streaming Support

| Thread | Agent Loop |
|--------|-----------|
| No streaming state | Tracks streaming messages |
| No pending tool calls | Tracks pending tool calls |

### 5. Child Thread Management

| Thread | Agent Loop |
|--------|-----------|
| Supports child threads | N/A |
| Cascade cancel | N/A |

### 6. State Transitor Pattern

| Thread | Agent Loop |
|--------|-----------|
| Dedicated ThreadStateTransitor | Inline in coordinator |
| Separate validator module | Inline checks |

## Common Patterns

### 1. Interrupt Flags

Both support pause/stop interrupts:

```typescript
// ThreadState
_shouldPause: boolean
_shouldStop: boolean

// AgentLoopState
_shouldPause: boolean
_shouldStop: boolean
```

### 2. Timing

Both track start/end time:

```typescript
_startTime: number | null
_endTime: number | null
```

### 3. Error Handling

Both track errors:

```typescript
_error: unknown
```

### 4. Lifecycle Events

Both emit lifecycle events:
- ThreadStarted/ThreadCompleted/ThreadFailed/etc.
- (similar for AgentLoop)

## State Machine Architecture

```
+----------------+     +-------------------+
|   ThreadState   |     | AgentLoopState    |
| - status       |     | - status          |
| - shouldPause  |     | - shouldPause     |
| - shouldStop   |     | - shouldStop     |
| - startTime    |     | - startTime      |
| - endTime     |     | - endTime       |
| - error       |     | - error         |
+----------------+     +-------------------+
         |                       |
         v                       v
+----------------+     +-------------------+
|ThreadStateTrans|    |(inline in coord) |
+----------------+     +-------------------+
         |
         v
+----------------+
| ThreadEntity   |
| - status       |
| - state       |
+----------------+
```

## When to Use Which

| Use Thread When | Use Agent Loop When |
|----------------|-------------------|
| Executing a workflow graph | Running an LLM loop with tools |
| Need child thread support | Need iteration tracking |
| Simple single-phase execution | Multi-turn conversation |
| Need timeout control | Need stop condition logic |
| No streaming requirements | Streaming LLM responses |

## Integration

### Agent Loop in Thread

AgentLoopEntity is typically executed within a Thread:

```
Thread (RUNNING)
  └── AgentLoopHandler
        └── AgentLoop (RUNNING)
              ├── Iteration 1
              │     ├── LLM call
              │     └── Tool calls
              └── Iteration N
                    └── (completed/failed)
```

### Checkpoint Interaction

Both support checkpoint:
- Thread: Via CheckpointStateManager
- AgentLoop: Via AgentLoopCheckpointCoordinator

## Summary

- **Thread State Machine**: Designed for workflow-level execution with child thread support
- **Agent Loop State Machine**: Designed for iteration-based LLM execution with tool call tracking

Both follow similar patterns (CREATED → RUNNING → terminal states) but differ in:
1. Iteration model (Thread: single, AgentLoop: multi)
2. Tool call tracking (AgentLoop only)
3. Streaming state (AgentLoop only)
4. Timeout handling (Thread: explicit, AgentLoop: via max iterations)