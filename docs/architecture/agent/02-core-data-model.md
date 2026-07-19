# Agent Core Data Model

## 1. AgentLoopEntity

`AgentLoopEntity` is the central runtime representation of a single agent loop execution. It implements `IExecutionEntity` and wraps three key components:

```
AgentLoopEntity
├── identity: id, executionId, nodeId
├── config: AgentLoopRuntimeConfig  (immutable, NOT serialized)
│   ├── profileId, systemPrompt, maxIterations
│   ├── hooks: AgentHook[]
│   ├── triggers: AgentTrigger[]
│   ├── availableTools: AgentToolConfig
│   ├── checkpointConfig: AgentCheckpointConfig
│   └── ... callbacks and configuration
│
├── state: AgentLoopState  (mutable, SERIALIZED)
│   ├── _status: AgentLoopStatus
│   ├── _currentIteration: number
│   ├── _toolCallCount: number
│   ├── _iterationHistory: IterationRecord[]
│   ├── _startTime, _endTime: number
│   ├── _error: ExecutionErrorRecord?
│   └── ... transient state (streaming, pending tool calls)
│
└── managers: (runtime-only, NOT serialized)
    ├── conversationSession: ConversationSession
    ├── executionHierarchyManager: ExecutionHierarchyManager
    ├── toolFailureProtectionState: ToolFailureProtectionState
    ├── interruptionState: InterruptionState
    ├── timeoutManager: TimeoutManager
    ├── triggerStateManager: TriggerStateManager
    └── retryBudget: RetryBudget
```

### Key Design Decisions

- **Config/State separation**: Config is immutable and contains functions; state is mutable and serializable
- **Manager lifecycle**: Managers are created at entity creation time and recreated on restore
- **No dual-write**: Messages are stored only in ConversationSession, eliminating synchronization issues
- **Encapsulation**: conversationManager is private, accessed via getConversationManager()

### Steering and Follow-up Modes

The entity supports configurable message processing modes:

- **Steering Mode**: `"one-at-a-time"` (default) or `"all"` — determines how steering messages are processed
- **Follow-up Mode**: `"one-at-a-time"` (default) or `"all"` — determines how follow-up messages are processed

## 2. AgentLoopRuntimeConfig

`AgentLoopRuntimeConfig` is the immutable configuration that defines agent loop behavior:

```
AgentLoopRuntimeConfig
├── agentConfigId?: ID
├── profileId?: ID
├── systemPrompt?: string
├── systemPromptTemplateId?: string
├── systemPromptTemplateVariables?: Record<string, unknown>
├── maxIterations?: number
├── initialMessages: Message[]
├── availableTools?: AgentToolConfig
├── stream: boolean
├── createCheckpointOnEnd: boolean
├── createCheckpointOnError: boolean
├── hooks: AgentHook[]
├── triggers: AgentTrigger[]
├── metadata: Record<string, unknown>
├── steeringMode: SteeringMode
├── followUpMode: FollowUpMode
└── retryPolicy?: RetryPolicy
```

### Design Notes

- Contains function references (hooks with callbacks), making it unserializable
- Created via `AgentLoopConfigBuilder` fluent API
- On checkpoint restore, the application re-provides the config

## 3. AgentLoopState

`AgentLoopState` is the ONLY serializable mutable state. It implements `StateManager<AgentLoopStateSnapshot>`:

```
AgentLoopState
├── createSnapshot() → AgentLoopStateSnapshot
├── restoreFromSnapshot(snapshot) → void
│
├── Persistent State (Serialized):
│   ├── _status: AgentLoopStatus
│   ├── _currentIteration: number
│   ├── _toolCallCount: number
│   ├── _iterationHistory: IterationRecord[]  (complete log)
│   ├── _startTime, _endTime: number
│   ├── _error: ExecutionErrorRecord?
│   └── _errorChainManager: ErrorChainManager
│
├── Transient State (Not Serialized):
│   ├── _streamMessage: string? (partial streaming)
│   ├── _pendingToolCalls: ToolCallRecord[] (in-flight)
│   ├── _shouldPause, _shouldStop: boolean
│   └── _executionRecordManager: ExecutionRecordManager
│
└── Status Management:
    ├── start(), complete(), fail(), pause(), resume(), cancel()
    └── getStatus(): AgentLoopStatus
```

### Status Transitions

```
CREATED → RUNNING → COMPLETED
                 → FAILED
                 → PAUSED → RUNNING → COMPLETED
                                     → FAILED
                                     → CANCELLED
```

## 4. AgentLoopResult

The result of an agent loop execution:

```
AgentLoopResult
├── status: AgentLoopStatus
├── messages: LLMMessage[]  (final conversation)
├── iterationCount: number
├── toolCallCount: number
├── tokenUsage: TokenUsageStats
├── startTime: number
├── endTime: number
├── error?: ExecutionErrorRecord
└── metadata: Record<string, unknown>
```

## 5. Supporting Types

### ToolCallRecord

Records each tool call made during execution:

```
ToolCallRecord
├── iteration: number
├── toolName: string
├── toolCallId: string
├── startTime: number
├── endTime: number
├── input: unknown
├── output: unknown
├── status: "success" | "error"
└── error?: string
```

### IterationRecord

Records each iteration of the loop:

```
IterationRecord
├── iteration: number
├── llmResponse: LLMResult
├── toolCalls: ToolCallRecord[]
├── tokenUsage: TokenUsageStats
├── startTime: number
├── endTime: number
├── status: "completed" | "failed" | "interrupted"
└── error?: ExecutionErrorRecord
```

### AgentLoopStatus

```
enum AgentLoopStatus {
  CREATED = "created",
  RUNNING = "running",
  PAUSED = "paused",
  COMPLETED = "completed",
  FAILED = "failed",
  CANCELLED = "cancelled",
}
```