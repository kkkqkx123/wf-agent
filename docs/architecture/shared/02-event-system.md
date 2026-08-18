# Shared Event System

## 1. Overview

The shared event system provides a publish-subscribe mechanism for execution coordination across both workflow and agent modules. It is used for state change notifications, not for logging.

## 2. EventRegistry

The central event bus:

```
EventRegistry
├── emit(event) → Promise<void>
│   ├── Publish event to all subscribers
│   ├── Handle errors gracefully (catch and log)
│   └── Return when all subscribers processed
│
├── subscribe(eventType, handler) → Subscription
│   ├── Register handler for specific event type
│   └── Return Subscription (for unsubscribe)
│
├── unsubscribe(subscription) → void
│   └── Remove event handler
│
├── getEventHistory() → Event[]
│   └── Query past events
│
└── clear() → void
    └── Remove all subscribers
```

### Event Structure

```typescript
interface Event {
  type: EventType;
  timestamp: number;
  source: string;
  payload: Record<string, unknown>;
  metadata?: Record<string, unknown>;
}
```

### Event Types

| Category | Events |
|----------|--------|
| **Workflow Lifecycle** | `WORKFLOW_STARTED`, `WORKFLOW_COMPLETED`, `WORKFLOW_FAILED`, `WORKFLOW_PAUSED`, `WORKFLOW_RESUMED`, `WORKFLOW_CANCELLED` |
| **Agent Lifecycle** | `AGENT_STARTED`, `AGENT_COMPLETED`, `AGENT_FAILED`, `AGENT_PAUSED`, `AGENT_RESUMED`, `AGENT_CANCELLED` |
| **Node/Iteration** | `NODE_EXECUTION_STARTED`, `NODE_EXECUTION_COMPLETED`, `NODE_EXECUTION_FAILED`, `AGENT_ITERATION_STARTED`, `AGENT_ITERATION_COMPLETED` |
| **LLM** | `MESSAGE_ADDED`, `TOKEN_USAGE_WARNING`, `CONVERSATION_STATE_CHANGED` |
| **Tool** | `TOOL_CALL_STARTED`, `TOOL_CALL_COMPLETED`, `TOOL_APPROVAL_REQUESTED`, `TOOL_EXECUTION_FAILED` |
| **Checkpoint** | `CHECKPOINT_CREATED`, `CHECKPOINT_RESTORED`, `CHECKPOINT_FAILED`, `CHECKPOINT_DELETED` |
| **Hook** | `HOOK_EXECUTED`, `NODE_CUSTOM_EVENT` |
| **Trigger** | `TRIGGER_MATCHED`, `TRIGGER_ACTION_EXECUTED` |
| **Variable** | `VARIABLE_CHANGED` |
| **System** | `SYSTEM_ERROR`, `SYSTEM_WARNING` |

## 3. Event Emission

### emit() Function

```typescript
async function emit(eventManager: EventRegistry, event: Event): Promise<void>
```

- Pure function with error handling
- Each event type has a dedicated builder function
- Errors in subscribers don't affect other subscribers

### Event Emission Flow

```
Component → emit(eventManager, event)
  ├── EventRegistry.emit(event)
  │   ├── Iterate subscribers for this event type
  │   ├── Execute each handler (async)
  │   ├── Catch and log handler errors
  │   └── Record event in history
  └── Return void
```

## 4. Execution Event Bus

A centralized event bus for execution-related events:

```
getExecutionEventBus() → ExecutionEventBus
├── emit(event) → void
├── subscribe(type, handler) → () => void (unsubscribe)
├── getHistory() → Event[]
└── clear() → void
```

## 5. Event Builders

Event builder functions create structured event objects:

### Structure

```typescript
function buildEventName(payload: EventPayload): Event {
  return {
    type: EventType.EVENT_NAME,
    timestamp: Date.now(),
    source: "component-name",
    payload,
  };
}
```

### Builder Modules

| Module | Events Built |
|--------|-------------|
| `workflow-execution-events` | Workflow lifecycle events |
| `agent-events` | Agent lifecycle events |
| `node-events` | Node execution events |
| `llm-events` | LLM and conversation events |
| `tool-events` | Tool call events |
| `checkpoint-events` | Checkpoint lifecycle events |
| `hook-events` | Hook execution events |
| `conversation-events` | Message and conversation events |
| `error-events` | Error events |
| `system-events` | System events |
| `subgraph-events` | Subgraph execution events |
| `custom-events` | Custom/user-defined events |
| `interaction-events` | User interaction events |
| `skill-events` | Skill execution events |
| `async-completion-events` | Async completion events |
| `attempt-completion-events` | Attempt completion events |

## 6. Event Subscription

### Subscription Pattern

```typescript
// Subscribe
const subscription = eventRegistry.subscribe(
  EventType.WORKFLOW_COMPLETED,
  async (event) => {
    console.log("Workflow completed:", event.payload.workflowId);
  }
);

// Unsubscribe
eventRegistry.unsubscribe(subscription);

// OR via returned function
const unsubscribe = executionEventBus.subscribe(
  EventType.AGENT_STARTED,
  handler
);
// Later:
unsubscribe();
```

## 7. Design Principles

- **Events are for state changes and coordination**, not for logging
- **Pure function emission**: `emit()` is a pure function with error handling
- **Decoupled**: Components communicate through events without direct dependencies
- **Extensible**: New event types can be added via plugins
- **History**: Events are recorded for debugging and analysis