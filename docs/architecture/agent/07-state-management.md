# Agent State Management

## 1. State Manager Architecture

The agent state management follows the same `StateManager<T>` pattern as workflow, providing unified snapshot/restore for checkpoint:

```typescript
interface StateManager<T> {
  createSnapshot(): T;
  restoreFromSnapshot(snapshot: T): void;
  cleanup(): void;
  reset(): void;
}
```

## 2. AgentLoopState

`AgentLoopState` is the primary state manager for agent loop execution. It manages the execution status, iteration history, and error records.

### State Structure

```
AgentLoopState
├── Status Management
│   ├── _status: AgentLoopStatus
│   ├── start() → void
│   ├── complete() → void
│   ├── fail(error) → void
│   ├── pause() → void
│   ├── resume() → void
│   └── cancel() → void
│
├── Iteration Tracking
│   ├── _currentIteration: number
│   ├── getCurrentIteration() → number
│   ├── incrementIteration() → void
│   └── _toolCallCount: number
│
├── History Management
│   ├── _iterationHistory: IterationRecord[]
│   ├── recordIteration(record) → void
│   ├── getIterationHistory() → IterationRecord[]
│   └── getLastIteration() → IterationRecord?
│
├── Error Management
│   ├── _error: ExecutionErrorRecord?
│   ├── setError(error) → void
│   ├── getError() → ExecutionErrorRecord?
│   └── _errorChainManager: ErrorChainManager
│
├── Execution Records
│   ├── _executionRecordManager: ExecutionRecordManager
│   └── recordEvent(event) → void
│
├── Timestamps
│   ├── _startTime: number
│   ├── _endTime: number
│   └── getDuration() → number
│
├── Token Usage
│   ├── _tokenUsage: TokenUsageStats
│   └── updateTokenUsage(usage) → void
│
└── Checkpoint Support
    ├── createSnapshot() → AgentLoopStateSnapshot
    └── restoreFromSnapshot(snapshot) → void
```

### Status Transitions

```
                    ┌──────────┐
                    │  CREATED │
                    └────┬─────┘
                         │ start()
                    ┌────▼─────┐
               ┌────│  RUNNING │────┐
               │    └────┬─────┘    │
               │         │          │
          pause()    complete()  fail()
               │         │          │
          ┌────▼───┐  ┌──▼───┐  ┌──▼───┐
          │ PAUSED │  │COMPL.│  │FAILED│
          └────┬───┘  └──────┘  └──────┘
               │
          resume()  /  cancel()
               │         │
          ┌────▼───┐  ┌──▼───┐
          │ RUNNING│  │CANCEL│
          └────────┘  └──────┘
```

### Error Management

The state includes error chain management via `ErrorChainManager`:

```
ErrorChainManager
├── recordError(errorRecord) → Add error to chain
├── getErrorChain(fromErrorId) → Traverse error chain
├── getRootCauseError() → Find originating error
├── getErrorCount() → Total errors recorded
└── getErrorHistory() → All errors in sequence
```

### Error Pattern Analysis

```typescript
interface ErrorPattern {
  type: 'none' | 'single' | 'chain';
  depth: number;
  rootCause?: ExecutionErrorRecord;
  patterns: string[];
  recommendation?: string;
}
```

## 3. AgentStateCoordinator

`AgentStateCoordinator` extends `BaseStateCoordinator` to provide unified message management for agent loops:

```
AgentStateCoordinator (extends BaseStateCoordinator<AgentStateSnapshot>)
├── Message Management (inherited)
│   ├── getConversationManager() → ConversationSession
│   ├── getMessages() → LLMMessage[]
│   ├── addMessage(msg) → void
│   └── getMessageCount() → number
│
├── Checkpoint Support (inherited)
│   ├── createSnapshot() → AgentStateSnapshot
│   └── restoreFromSnapshot(snapshot) → void
│
└── Parent-child messaging (inherited)
    ├── exportMessagesForChild() → messages
    ├── importMessagesFromChild() → void
    └── exportAllMessagesForCheckpoint() → messages
```

### Design Principles

- **Single data source**: Messages are managed by the coordinator, eliminating dual-write issues
- **Checkpoint-compatible**: State serialization for checkpoint/restore
- **Created by AgentLoopCoordinator**: During entity creation
- **Stored in AgentLoopRegistry**: Alongside the entity

## 4. State Snapshot (Checkpoint)

The `AgentLoopStateSnapshot` is the serializable form of the execution state:

```
AgentLoopStateSnapshot
├── status: AgentLoopStatus
├── currentIteration: number
├── toolCallCount: number
├── iterationHistory: IterationRecord[]
├── startTime: number
├── endTime: number?
├── error: ExecutionErrorRecord?
├── errorChain: ExecutionErrorRecord[]
├── executionRecords: ExecutionEventRecord[]
├── interruptionRecords: ExecutionInterruptionRecord[]
└── tokenUsage: TokenUsageStats
```

## 5. Execution Records

The `ExecutionRecordManager` tracks execution events:

```
ExecutionRecordManager
├── recordEvent(event) → void
├── getEvents() → ExecutionEventRecord[]
├── getEventsByType(type) → ExecutionEventRecord[]
├── getEventsByRange(startTime, endTime) → ExecutionEventRecord[]
└── createSnapshot() → ExecutionEventRecord[]
```

### Event Record Types

| Event Type | Description |
|-----------|-------------|
| `ITERATION_START` | Iteration began |
| `ITERATION_COMPLETE` | Iteration ended |
| `LLM_CALL_START` | LLM call started |
| `LLM_CALL_COMPLETE` | LLM call ended |
| `TOOL_CALL_START` | Tool call started |
| `TOOL_CALL_COMPLETE` | Tool call ended |
| `STATE_CHANGE` | Status changed (e.g., RUNNING → PAUSED) |
| `ERROR` | Error occurred |

## 6. State Coordination

The `AgentLoopStateTransitor` provides atomic state transition operations:

```
AgentLoopStateTransitor
├── startAgentLoop(entity, messageCount) → void
│   ├── Validates transition (CREATED → RUNNING)
│   ├── Updates entity state
│   └── Emits AGENT_STARTED event
│
├── completeAgentLoop(entity, result) → void
│   ├── Validates transition (RUNNING → COMPLETED)
│   ├── Updates entity state
│   └── Emits AGENT_COMPLETED event
│
├── failAgentLoop(entity, error, result) → void
│   ├── Sets error state
│   └── Emits AGENT_FAILED event
│
├── pauseAgentLoop(entity) → void
│   └── Emits AGENT_PAUSED event
│
├── resumeAgentLoop(entity) → void
│   └── Emits AGENT_RESUMED event
│
└── cancelAgentLoop(entity) → void
    └── Emits AGENT_CANCELLED event
```

### Design Principles

- **Atomic operations**: Each method is a complete state transition unit
- **Process orchestration**: Manages complex multi-step operations
- **Delegation pattern**: Coordinates multiple components
- **Entity encapsulation**: Never directly access entity data, use entity methods