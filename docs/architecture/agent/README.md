# Agent Architecture Index

## Document Map

| # | Document | Description |
|---|----------|-------------|
| 01 | [Overview and Architecture](01-overview-and-architecture.md) | High-level architecture, module boundaries, execution model |
| 02 | [Core Data Model](02-core-data-model.md) | AgentLoopEntity, AgentLoopState, AgentLoopRuntimeConfig |
| 03 | [Builder and Configuration](03-builder-and-configuration.md) | Instance creation, config resolution, factory patterns |
| 04 | [Execution Engine](04-execution-engine.md) | Coordinator-executor-handler pattern, lifecycle management |
| 05 | [Iteration and Tool Execution](05-iteration-and-tool-execution.md) | Iteration flow, LLM calls, tool execution, conversation coordination |
| 06 | [Checkpoint Mechanism](06-checkpoint-mechanism.md) | State snapshot, restoration, checkpoint policies |
| 07 | [State Management](07-state-management.md) | AgentLoopState, AgentStateCoordinator, state transitions |
| 08 | [Interruption and Error Handling](08-interruption-and-error-handling.md) | Pause/resume/stop, error chains, retry with backoff |
| 09 | [Execution Hierarchy](09-execution-hierarchy.md) | Parent-child relationships, triggered execution |
| 10 | [Event System and Hooks](10-event-system-and-hooks.md) | Agent events, hook execution lifecycle |
| 11 | [Trigger System](11-trigger-system.md) | Agent triggers, trigger execution handlers |
| 12 | [API and Command Layer](12-api-and-command-layer.md) | Command pattern, resource APIs, fluent builders |
| 13 | [DI and Plugin System](13-di-and-plugin-system.md) | Service registration, agent services in DI |
| 14 | [Registry and Storage](14-registry-and-storage.md) | AgentLoopRegistry, execution registry, storage adapters |
| 15 | [Validation System](15-validation-system.md) | Config validation, protocol validation, tool format compatibility |

## Shared Infrastructure

The following shared modules are used by both Agent and Workflow architectures. See the [Shared Architecture](../shared/) directory for details.

| # | Document | Description |
|---|----------|-------------|
| 01 | [Checkpoint Core](../shared/01-checkpoint-core.md) | BaseCheckpointCoordinator, BaseCheckpointStateManager, hierarchy restore |
| 02 | [Event System](../shared/02-event-system.md) | EventRegistry, event emission, execution event bus |
| 03 | [Messaging](../shared/03-messaging.md) | ConversationSession, message management, state coordination |
| 04 | [Common Coordinators](../shared/04-coordinators.md) | LLMExecutionCoordinator, ToolApprovalCoordinator, RetryBudget |
| 05 | [Registry Base](../shared/05-registry-base.md) | ExecutionStore, CoordinatorStore, TaskRegistry |
| 06 | [Execution Hierarchy](../shared/06-execution-hierarchy.md) | ExecutionHierarchyManager, HierarchyRegistry, HierarchyIntegrityService |
| 07 | [Interruption System](../shared/07-interruption.md) | InterruptionState, AbortController, interruption utilities |
| 08 | [Protection Mechanisms](../shared/08-protection.md) | TimeoutManager, ToolFailureProtectionState |
| 09 | [Error Management](../shared/09-errors.md) | ErrorChainManager, error utilities |
| 10 | [Tool System](../shared/10-tools.md) | ToolRegistry, tool schema helpers, tool declaration formatter |
| 11 | [Hooks System](../shared/11-hooks.md) | HookExecutor, hook types, hook execution |
| 12 | [Event Builders](../shared/12-event-builders.md) | Event builder functions for all event types |

## Key Design Principles

### 1. Separation of Concerns

The architecture is organized into clear layers:

```
API Layer (Commands, Resources, Builders)
    ↓
Coordination Layer (Coordinators)
    ↓
Execution Layer (Executors, Handlers)
    ↓
State Management Layer (State Managers)
    ↓
Data Layer (Entities, Registries)
```

### 2. Coordinator-Executor-Handler Pattern

- **Coordinators**: Orchestrate interactions between components (stateless, DI-injected)
- **Executors**: Execute specific tasks (stateless, focused)
- **Handlers**: Handle specific concerns (hooks, triggers, errors)

### 3. Immutable Config, Mutable State

- `AgentLoopRuntimeConfig` is immutable after construction
- `AgentLoopState` is the only serializable mutable state
- Runtime managers (`ConversationSession`, etc.) are recreated on restore

### 4. State Manager Pattern

- Each state domain has a dedicated `StateManager<T>` implementation
- Unified `createSnapshot()` / `restoreFromSnapshot()` for checkpoint
- Clean separation of persistent and transient state

### 5. Event-Driven Coordination

- Events for state changes, not logging
- Decoupled communication between components
- Extensible via hook subscriptions

### 6. Layered Execution Model

```
AgentLoopCoordinator (lifecycle)
  └─ AgentLoopExecutor (stateless execution)
       └─ AgentExecutionCoordinator (main loop)
            └─ AgentIterationCoordinator (per-iteration)
                 ├─ LLMExecutionCoordinator (shared)
                 ├─ ToolExecutionCoordinator (tool calls)
                 ├─ HookHandler (hooks)
                 └─ TriggerHandler (triggers)
```

## Execution Flow Summary

```
1. Define Agent Config (AgentDefinitionBuilder / config file)
2. Create Entity (AgentLoopFactory.create)
3. Execute (AgentLoopCoordinator.execute)
   ├── AgentLoopExecutor.execute()
   │   └── AgentExecutionCoordinator.execute()
   │       └── Loop iterations:
   │           └── AgentIterationCoordinator.executeIteration()
   │               ├── BEFORE_ITERATION hooks
   │               ├── BEFORE_LLM_CALL hooks
   │               ├── LLMExecutionCoordinator.execute()
   │               ├── AFTER_LLM_CALL hooks
   │               ├── ToolExecutionCoordinator.executeTools()
   │               │   ├── BEFORE_TOOL_CALL hooks
   │               │   ├── ToolCallExecutor.execute()
   │               │   └── AFTER_TOOL_CALL hooks
   │               └── AFTER_ITERATION hooks
   └── AgentLoopStateTransitor.completeAgentLoop()
4. Return AgentLoopResult
```

## Data Flow

```
Input (messages, config) → AgentLoopEntity
  ├── Config (AgentLoopRuntimeConfig - immutable)
  ├── State (AgentLoopState - mutable, serializable)
  └── Managers (ConversationSession, TimeoutManager, etc.)
       └── Loop iterations
            ├── LLM calls (produce responses)
            ├── Tool calls (produce results)
            └── State updates (iteration history, status)
                 └── Output (AgentLoopResult)
```