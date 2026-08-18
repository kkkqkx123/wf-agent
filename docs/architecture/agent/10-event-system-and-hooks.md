# Agent Event System and Hooks

## 1. Event System

The agent event system builds on the shared event infrastructure with agent-specific event types.

### Agent Event Types

| Event Category | Events |
|---------------|--------|
| **Agent Lifecycle** | `AGENT_STARTED`, `AGENT_COMPLETED`, `AGENT_FAILED`, `AGENT_PAUSED`, `AGENT_RESUMED`, `AGENT_CANCELLED` |
| **Iteration** | `AGENT_ITERATION_STARTED`, `AGENT_ITERATION_COMPLETED` |
| **LLM** | `AGENT_LLM_CALL_STARTED`, `AGENT_LLM_CALL_COMPLETED`, `MESSAGE_ADDED` |
| **Tool** | `AGENT_TOOL_EXECUTION_STARTED`, `AGENT_TOOL_EXECUTION_COMPLETED` |
| **Hook** | `AGENT_HOOK_TRIGGERED` |
| **Checkpoint** | `CHECKPOINT_CREATED`, `CHECKPOINT_RESTORED` |
| **Attempt Completion** | `ATTEMPT_COMPLETION_EVENT` |

### Event Builders

Agent-specific event builders are in `shared/events/builders/agent-events.ts`:

| Builder Function | Event Type |
|-----------------|-----------|
| `buildAgentStartedEvent()` | `AGENT_STARTED` |
| `buildAgentCompletedEvent()` | `AGENT_COMPLETED` |
| `buildAgentFailedEvent()` | `AGENT_FAILED` |
| `buildAgentPausedEvent()` | `AGENT_PAUSED` |
| `buildAgentResumedEvent()` | `AGENT_RESUMED` |
| `buildAgentCancelledEvent()` | `AGENT_CANCELLED` |
| `buildAgentIterationCompletedEvent()` | `AGENT_ITERATION_COMPLETED` |
| `buildAgentToolExecutionStartedEvent()` | `AGENT_TOOL_EXECUTION_STARTED` |
| `buildAgentToolExecutionCompletedEvent()` | `AGENT_TOOL_EXECUTION_COMPLETED` |
| `buildMessageAddedEvent()` | `MESSAGE_ADDED` |
| `buildAttemptCompletionEvent()` | `ATTEMPT_COMPLETION_EVENT` |

### Event Emission Flow

Events are emitted via the shared `emit()` utility:

```typescript
import { emit } from "../../shared/events/emit-event.js";

// Example: emit agent started event
const event = buildAgentStartedEvent({
  agentLoopId: entity.id,
  maxIterations: entity.config.maxIterations ?? -1,
  initialMessageCount: messageCount,
  executionId: entity.id,
});
await emit(eventManager, event);
```

## 2. Hook System

### Agent Hook Types

The agent loop supports the following hook lifecycle points:

| Hook Type | Timing | Context |
|-----------|--------|---------|
| `BEFORE_ITERATION` | Before iteration starts | Agent loop entity, config |
| `AFTER_ITERATION` | After iteration ends | Agent loop entity, iteration result |
| `BEFORE_LLM_CALL` | Before LLM call | Messages, LLM profile |
| `AFTER_LLM_CALL` | After LLM call | LLM result, token usage |
| `BEFORE_TOOL_CALL` | Before tool call starts | Tool call details |
| `AFTER_TOOL_CALL` | After tool call ends | Tool result, execution time |

### Hook Execution

The `executeAgentHook()` function handles hook execution:

```
executeAgentHook(hookType, entity, context):
  1. Find matching hooks for the hook type
  2. For each matching hook:
     a. Build evaluation context
     b. Evaluate hook condition (if any)
     c. If condition matches:
        - Execute hook action (callback)
        - Emit AGENT_HOOK_TRIGGERED event
        - Return hook result
  3. Return aggregated hook results
```

### Hook Context

```typescript
interface AgentHookExecutionContext {
  entity: AgentLoopEntity;
  hookType: HookType;
  messages?: LLMMessage[];
  llmResult?: LLMResult;
  toolCalls?: ToolCallRecord[];
  toolResults?: ToolExecutionResult[];
  iteration?: number;
  metadata?: Record<string, unknown>;
}
```

### Hook Evaluation Context

The `buildAgentHookEvaluationContext()` function builds the evaluation context for condition checking:

```
AgentHookEvaluationContext:
├── agentLoopId: ID
├── currentIteration: number
├── totalIterations: number
├── status: AgentLoopStatus
├── toolCallCount: number
├── conversationLength: number
├── lastMessage?: LLMMessage
├── lastToolCall?: ToolCallRecord
└── metadata: Record<string, unknown>
```

### Hook Configuration

```typescript
interface AgentHook {
  type: HookType;
  condition?: HookCondition;  // Optional condition expression
  action: HookAction;         // Callback function
  priority?: number;          // Execution order (lower = earlier)
}
```

### Hook Event Emission

Hook events are emitted via `emitAgentHookEvent()`:

```
emitAgentHookEvent(hookType, entity, context):
  1. Build event payload
  2. Emit AGENT_HOOK_TRIGGERED event
  3. Include hook type, agent loop ID, iteration, metadata
```

## 3. Hook Execution Flow within Iteration

```
AgentIterationCoordinator.executeIteration():
  1. executeAgentHook(BEFORE_ITERATION, entity, context)
  2. Prepare messages
  3. executeAgentHook(BEFORE_LLM_CALL, entity, context)
  4. LLMExecutionCoordinator.execute()
  5. executeAgentHook(AFTER_LLM_CALL, entity, context)
  6. Process response:
     if tool calls:
       for each tool call:
         a. executeAgentHook(BEFORE_TOOL_CALL, entity, context)
         b. Execute tool
         c. executeAgentHook(AFTER_TOOL_CALL, entity, context)
  7. executeAgentHook(AFTER_ITERATION, entity, context)
```

## 4. Event-Driven Coordination

The event system enables decoupled communication between components:

```
Component A → emit(event) → EventRegistry
                              ├── Subscriber 1 (logging)
                              ├── Subscriber 2 (metrics)
                              └── Subscriber 3 (external notification)
```

### Execution Event Bus

The `getExecutionEventBus()` provides a centralized event bus for execution-related events:

```
ExecutionEventBus
├── emit(event) → void
├── subscribe(type, handler) → unsubscribe function
└── getHistory() → Event[]
```

### Event Usage

- **Events are for state changes and coordination**, not for logging
- Pure function with error handling
- Each event type has a dedicated builder function
- Events can be subscribed to via plugins for extensibility