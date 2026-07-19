# Workflow Architecture Index

## Document Map

| # | Document | Description |
|---|----------|-------------|
| 01 | [Overview and Architecture](01-overview-and-architecture.md) | High-level architecture, module boundaries, execution model |
| 02 | [Core Data Model](02-core-data-model.md) | WorkflowGraph, WorkflowExecutionEntity, Node Type System |
| 03 | [Builder and Navigator](03-builder-and-navigator.md) | Graph construction, preprocessing, navigation |
| 04 | [Execution Engine](04-execution-engine.md) | Coordinator-executor-handler pattern, lifecycle management |
| 05 | [Node Handlers](05-node-handlers.md) | All 16+ node handler types and their behavior |
| 06 | [Checkpoint Mechanism](06-checkpoint-mechanism.md) | State snapshot, restoration, checkpoint strategies |
| 07 | [State Management](07-state-management.md) | Execution state, fork/join state, variables, interruption |
| 08 | [Interruption and Error Handling](08-interruption-and-error-handling.md) | Pause/resume/stop, error chains, timeout management |
| 09 | [Execution Hierarchy](09-execution-hierarchy.md) | Parent-child relationships, hierarchy registry |
| 10 | [Event System and Hooks](10-event-system-and-hooks.md) | Event bus, hook execution, SyncBarrier |
| 11 | [SDK Kit Layer](11-sdk-kit-layer.md) | Higher-level API, fluent builders, Result pattern |
| 12 | [API and Command Layer](12-api-and-command-layer.md) | Command pattern, resource APIs, validators |
| 13 | [DI Container and Plugin System](13-di-and-plugin-system.md) | Service registration, plugin contributions |
| 14 | [Registry and Storage](14-registry-and-storage.md) | Runtime registries, storage adapters, metrics |
| 15 | [Validation System](15-validation-system.md) | Graph validation, node validation, config validation |

## Key Design Principles

### 1. Separation of Concerns

The architecture is organized into clear layers:

```
API Layer (Commands, Resources)
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
- **Handlers**: Handle specific node types (pluggable, extensible)

### 3. Immutable Graph Structure

- `WorkflowGraphStructure` is immutable after construction
- Preprocessing data stored separately in `WorkflowGraphMetadata`
- Runtime reads from the graph without modification

### 4. State Manager Pattern

- Each state domain has a dedicated `StateManager<T>` implementation
- Unified `createSnapshot()` / `restoreFromSnapshot()` for checkpoint
- Clean separation of persistent and transient state

### 5. Event-Driven Coordination

- Events for state changes, not logging
- Decoupled communication between components
- Extensible via plugin subscriptions

### 6. Node Type Evolution

- **StaticNode** types for definition (TOML/config)
- **RuntimeNode** types for execution (after preprocessing)
- **WorkflowNode** combines runtime node with original reference
- SUBGRAPH nodes remain at runtime (independent entities)
- EMBED_GRAPH nodes are expanded during preprocessing

## Execution Flow Summary

```
1. Define Workflow (TOML / WorkflowBuilder)
2. Build Graph (WorkflowGraphBuilder.buildAndValidate)
3. Preprocess (subgraph expansion, topological sort)
4. Create Execution Entity (WorkflowExecutionBuilder.build)
5. Execute (WorkflowLifecycleCoordinator.execute)
   ├── WorkflowExecutor.executeWorkflow()
   │   └── WorkflowExecutionCoordinator.execute()
   │       └── NodeExecutionCoordinator.executeNode()
   │           ├── NodeHandlerContextFactory.createHandlerContext()
   │           ├── HookHandler (before)
   │           ├── NodeCheckpointStrategy (before)
   │           ├── getNodeHandler() → NodeHandlerFn()
   │           ├── NodeCheckpointStrategy (after)
   │           └── HookHandler (after)
   └── WorkflowStateTransitor.completeWorkflowExecution()
6. Return Result
```

## Data Flow

```
Input → WorkflowExecutionEntity
         ├── Variables (VariableManager)
         ├── Messages (ConversationSession)
         └── Graph (WorkflowGraph - immutable)
              └── Nodes (iterated by WorkflowNavigator)
                   └── Node Handlers (produce NodeExecutionResults)
                        └── State Updates (status, variables, messages)
                             └── Output (WorkflowExecutionResult)
```