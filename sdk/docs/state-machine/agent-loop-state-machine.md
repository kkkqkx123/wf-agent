# Agent Loop State Machine

The Agent Loop state machine manages the execution lifecycle of agent loop instances. It follows an iteration-based execution model with streaming state tracking.

## Status Definition

### AgentLoopStatus Enum

| Status | Description | Type |
|--------|-------------|------|
| `CREATED` | Agent loop created, not started | Initial |
| `RUNNING` | Agent loop is executing (iterating) | Active |
| `PAUSED` | Agent loop is paused (can be resumed) | Active |
| `COMPLETED` | Agent loop completed successfully | Terminal (Success) |
| `FAILED` | Agent loop execution failed | Terminal (Error) |
| `CANCELLED` | Agent loop was cancelled | Terminal (Error) |

**Source**: `packages/types/src/agent/status.ts`

## Status Transition Rules

```
CREATED  → RUNNING
RUNNING  → PAUSED | COMPLETED | FAILED | CANCELLED
PAUSED  → RUNNING | CANCELLED
COMPLETED/FAILED/CANCELLED → (final states, no further transitions)
```

### Transition Table

| From \ To | CREATED | RUNNING | PAUSED | COMPLETED | FAILED | CANCELLED |
|-----------|---------|---------|-------|----------|--------|----------|
| CREATED   | -       | ✓      | -     | -         | -      | -         |
| RUNNING  | -       | -      | ✓     | ✓        | ✓      | ✓         |
| PAUSED   | -       | ✓      | -     | -         | -      | ✓         |
| COMPLETED| -       | -      | -     | -         | -      | -         |
| FAILED   | -       | -      | -     | -         | -      | -         |
| CANCELLED| -       | -      | -     | -         | -      | -         |

## Differences from Thread State Machine

| Aspect | Thread | Agent Loop |
|--------|--------|------------|
| TIMEOUT status | Yes | No |
| Iteration tracking | No | Yes (iteration count, history) |
| Tool call tracking | No | Yes (pending, completed) |
| Stream state | No | Yes (partial messages) |
| Iteration model | Single execution | Multi-iteration loop |

## Implementation Components

### 1. AgentLoopState (State Manager)

**Location**: `sdk/agent/entities/agent-loop-state.ts`

Manages runtime state during agent loop execution:

**Basic State**:
| Field | Type | Description |
|-------|------|-------------|
| `_status` | AgentLoopStatus | Current status |
| `_currentIteration` | number | Current iteration count |
| `_toolCallCount` | number | Total tool calls |
| `_iterationHistory` | IterationRecord[] | All iteration records |
| `_currentIterationRecord` | IterationRecord | Current iteration data |
| `_startTime` | number | Execution start timestamp |
| `_endTime` | number | Execution end timestamp |
| `_error` | unknown | Error message |

**Interrupt Flags**:
| Field | Type | Description |
|-------|------|-------------|
| `_shouldPause` | boolean | Pause interrupt flag |
| `_shouldStop` | boolean | Stop interrupt flag |

**Streaming State (NEW)**:
| Field | Type | Description |
|-------|------|-------------|
| `_streamMessage` | LLMMessage | Partial message during streaming |
| `_pendingToolCalls` | Set<string> | Pending tool call IDs |
| `_isStreaming` | boolean | Streaming flag |

**Key Methods**:
| Method | Description |
|--------|-------------|
| `start()` | Transition to RUNNING |
| `startIteration()` | Start new iteration |
| `endIteration()` | End current iteration |
| `recordToolCallStart()` | Record tool call start |
| `recordToolCallEnd()` | Record tool call end |
| `startStreaming()` | Start streaming |
| `updateStreamMessage()` | Update stream message |
| `endStreaming()` | End streaming |
| `pause()` | Transition to PAUSED |
| `resume()` | Transition to RUNNING |
| `complete()` | Transition to COMPLETED |
| `fail(error)` | Transition to FAILED |
| `cancel()` | Transition to CANCELLED |

### 2. IterationRecord

Represents a single iteration:

```typescript
interface IterationRecord {
  iteration: number;
  startTime: number;
  endTime?: number;
  responseContent?: string;
  toolCalls: ToolCallRecord[];
}
```

### 3. ToolCallRecord

Represents a single tool call:

```typescript
interface ToolCallRecord {
  id: string;
  name: string;
  arguments: unknown;
  startTime: number;
  endTime?: number;
  result?: unknown;
  error?: string;
}
```

## State Validation

Unlike Thread, Agent Loop uses inline validation in `AgentLoopState`:

```typescript
// No separate validator - transitions checked in state methods
start(): void {
  this._status = AgentLoopStatus.RUNNING;
}
```

Validation is enforced at execution coordinator level:

**Location**: `sdk/agent/execution/coordinators/agent-loop-coordinator.ts`

## Streaming State Tracking

Inspired by pi-agent-core, AgentLoopState tracks streaming state:

### Flow

1. **LLM starts streaming**: `startStreaming()`
2. **Partial message received**: `updateStreamMessage(delta)`
3. **Tool call detected**: `addPendingToolCall(toolCallId)`
4. **Tool execution completes**: `removePendingToolCall(toolCallId)`
5. **Streaming ends**: `endStreaming()` returns final message

### Pending Tool Calls

- Tracks tool calls in progress
- `isToolCallPending(id)` - Check if pending
- `getPendingToolCallCount()` - Get count

## Snapshot and Restore

### Snapshot (for Checkpoint)

**Location**: `agent-loop-state.ts:createSnapshot()`

```typescript
createSnapshot(): AgentLoopStateSnapshot {
  return {
    status: this._status,
    currentIteration: this._currentIteration,
    toolCallCount: this._toolCallCount,
    startTime: this._startTime,
    endTime: this._endTime,
    error: this._error,
    iterationHistory: this._iterationHistory,
    isStreaming: this._isStreaming,
    pendingToolCalls: Array.from(this._pendingToolCalls),
  };
}
```

### Restore (from Checkpoint)

**Location**: `agent-loop-state.ts:restoreFromSnapshot()`

- Restores status, iteration, timing, error
- Rebuilds iteration history
- Restores pending tool calls
- Resets runtime-only fields (current record, interrupt flags, stream message)

## Agent Loop Entity Integration

**Location**: `sdk/agent/entities/agent-loop-entity.ts`

The `AgentLoopEntity` encapsulates:

```typescript
class AgentLoopEntity {
  readonly id: string;
  config: AgentLoopConfig;
  state: AgentLoopState;
  conversationManager: ConversationManager;
  variableStateManager: VariableStateManager;
  abortController?: AbortController;
}
```

## Lifecycle Management

**Location**: `sdk/agent/execution/handlers/agent-loop-lifecycle.ts`

| Operation | Description |
|-----------|-------------|
| `createAgentLoopCheckpoint()` | Create checkpoint |
| `cleanupAgentLoop()` | Clean up resources |
| `cloneAgentLoop()` | Clone entity for resumption |

## Usage in Execution Flow

1. **Creation**: Status = `CREATED`
2. **Start**: `start()` -> `RUNNING`, iteration = 1
3. **Each Iteration**:
   - `startIteration()` - Start new iteration
   - LLM call with streaming
   - Tool calls tracked
   - `endIteration()` - End iteration
4. **Completion**: Determined by max iterations or stop condition
5. **Final State**: Terminal state set

## Event Integration

State transitions in AgentLoopCoordinator emit events:
- Iteration started/completed
- Tool call started/completed
- State changed
- Execution completed/failed

## Notes

- No separate TIMEOUT status (handled via max iterations)
- Iteration history provides full audit trail
- Streaming state enables real-time UI updates
- Pending tool calls support pause during tool execution
- Snapshot/restore enables checkpoint resumption