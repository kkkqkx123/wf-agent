# Event System and Hooks

## 1. Event System Architecture

The event system provides a publish-subscribe mechanism for workflow execution coordination.

### Event Registry

`EventRegistry` is the central event bus:

```
EventRegistry
├── emit(event) → Publish event to subscribers
├── subscribe(eventType, handler) → Register event handler
├── unsubscribe(eventType, handler) → Remove event handler
└── getEventHistory() → Query past events
```

### Event Types

| Event Category | Events |
|---------------|--------|
| **Workflow Lifecycle** | `WORKFLOW_STARTED`, `WORKFLOW_COMPLETED`, `WORKFLOW_FAILED`, `WORKFLOW_PAUSED`, `WORKFLOW_RESUMED`, `WORKFLOW_CANCELLED` |
| **Node Execution** | `NODE_EXECUTION_STARTED`, `NODE_EXECUTION_COMPLETED`, `NODE_EXECUTION_FAILED`, `NODE_EXECUTION_SKIPPED` |
| **Variable** | `VARIABLE_CHANGED` |
| **Checkpoint** | `CHECKPOINT_CREATED`, `CHECKPOINT_RESTORED` |
| **Hook** | `HOOK_EXECUTED`, `NODE_CUSTOM_EVENT` |
| **LLM** | `MESSAGE_ADDED`, `TOKEN_USAGE_WARNING`, `CONVERSATION_STATE_CHANGED` |
| **Tool** | `TOOL_CALL_STARTED`, `TOOL_CALL_COMPLETED`, `TOOL_APPROVAL_REQUESTED` |
| **Trigger** | `TRIGGER_MATCHED`, `TRIGGER_ACTION_EXECUTED` |

### Event Emission

Events are emitted via the `emit()` utility function:

```typescript
async function emit(eventManager: EventRegistry, event: Event): Promise<void>
```

- Events are for **state changes and coordination**, not for logging
- Pure function with error handling
- Each event type has a dedicated builder function

## 2. Hook System

### Hook Types

| Hook Type | Timing | Context |
|-----------|--------|---------|
| `BEFORE_EXECUTE` | Before node execution | Node config, execution context |
| `AFTER_EXECUTE` | After node execution | Node result, execution context |
| `ON_ERROR` | On node execution error | Error details, node context |
| `ON_COMPLETE` | On workflow completion | Final execution result |

### Hook Execution

```
HookExecutionContext:
├── workflowExecutionEntity: WorkflowExecutionEntity
├── node: StaticNode
├── result?: NodeExecutionResult (for AFTER_EXECUTE)
├── checkpointDependencies?: CheckpointDependencies
└── conversationManager: ConversationSession

Execution Flow:
1. filterAndSortHooks() → Filter hooks by criteria, sort by priority
2. buildHookEvaluationContext() → Build evaluation context
3. executeHooks() → Execute matching hooks in order
4. emitHookEvent() → Emit hook custom events
```

### Hook Handler Resolution

Hooks are resolved via the `HookTemplateRegistry`:

- Template-based hooks defined in configuration
- Loaded and resolved at runtime
- Configurable conditions and actions

## 3. Event-Driven Coordination

The event system enables:

- **Checkpoint triggers**: React to execution events for checkpoint creation
- **Trigger matching**: Match workflow triggers against runtime events
- **Observability**: Track execution progress via event history
- **Plugin integration**: Plugins can subscribe to execution events
- **Cross-execution coordination**: Parent-child execution communication via events

## 4. SyncBarrier

`SyncBarrier` provides synchronization between fork branches:

```
SyncBarrier
├── waitForAll(branchIds) → Wait for all branches to reach sync point
├── waitForCount(n) → Wait for N branches to reach sync point
└── signal(branchId) → Signal that a branch has reached the sync point
```

- Used by `SYNC` nodes for explicit branch synchronization
- Supports timeout for deadlock prevention