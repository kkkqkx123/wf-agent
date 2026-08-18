# Workflow Architecture Overview

## 1. Introduction

The Modular Agent Framework provides a workflow execution engine that supports directed acyclic graph (DAG) based workflow orchestration. The system is designed as a layered architecture, separating concerns across the SDK core, the toolkit layer, and the type system.

### Architecture Layers

```
┌─────────────────────────────────────────────────────┐
│                    SDK Kit Layer                      │
│  (SDKKit, WorkflowBuilder, ExecutionRunner, QueryAPI)│
├─────────────────────────────────────────────────────┤
│                  SDK Core Layer                       │
│  ┌─────────────────────────────────────────────────┐ │
│  │          API Layer (Commands, Resources)          │ │
│  ├─────────────────────────────────────────────────┤ │
│  │        Workflow Execution Engine                  │ │
│  │  (Builder, Navigator, Coordinators, Executors,    │ │
│  │   Handlers, State Managers, Checkpoint)            │ │
│  ├─────────────────────────────────────────────────┤ │
│  │    Shared Services (LLM, Tools, Events, Hooks)    │ │
│  ├─────────────────────────────────────────────────┤ │
│  │   DI Container & Plugin System                    │ │
│  └─────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────┤
│                Type System (types package)             │
│  (StaticNode, RuntimeNode, WorkflowGraph, Execution)  │
└─────────────────────────────────────────────────────┘
```

## 2. Module Boundaries

### packages/sdk/workflow/ — Core Workflow Engine

Sub-modules:

| Module | Responsibility |
|--------|---------------|
| `entities/` | Core data entities: `WorkflowGraph`, `WorkflowExecutionEntity` |
| `builder/` | Graph construction, validation, navigation (`WorkflowGraphBuilder`, `WorkflowNavigator`) |
| `execution/` | Execution engine: coordinators, handlers, executors, factories |
| `state-managers/` | State management: `WorkflowExecutionState`, `ForkJoinState`, `ExecutionState` |
| `checkpoint/` | State snapshot and restoration: `CheckpointCoordinator` |
| `registry/` | Runtime registries: `WorkflowRegistry`, `WorkflowGraphRegistry`, `WorkflowExecutionRegistry` |
| `validation/` | Graph and node validation |

### packages/sdk-kit/ — SDK Toolkit

A higher-level API layer providing fluent builder patterns and Result-based error handling:

| Module | Responsibility |
|--------|---------------|
| `builders/` | `WorkflowBuilder` — programmatic workflow definition |
| `executors/` | `ExecutionRunner`, `QueryExecutor` — simplified execution API |
| `api/` | `WorkflowAPI`, `ExecutionAPI`, `ResourceAPI` — unified facade |
| `managers/` | `EventManager`, `ResourceManager` — lifecycle management |
| `types/` | Type definitions for the kit layer |
| `analysis/` | `ComparisonAnalysis`, `ProgressAnalysis` — workflow analysis |

### packages/types/ — Type System

Defines all type contracts used across the SDK:

- `StaticNode` types — for workflow definition (TOML/config)
- `RuntimeNode` types — for execution after preprocessing
- `WorkflowGraph` types — graph structure, edges, adjacency
- `WorkflowExecution` types — execution status, context, variables
- `Checkpoint` types — snapshot serialization
- `Trigger`, `Hook`, `Event` types — extensibility

## 3. Execution Model

The workflow execution follows a **coordinator-executor-handler** pattern:

```
WorkflowLifecycleCoordinator
  └─ WorkflowExecutor (stateless, single execution)
       └─ WorkflowExecutionCoordinator (main loop)
            └─ NodeExecutionCoordinator (per-node orchestration)
                 ├─ getNodeHandler() → NodeHandlerFn
                 ├─ NodeHandlerContextFactory → context
                 ├─ HookHandler (before/after hooks)
                 ├─ NodeCheckpointStrategy (checkpoints)
                 └─ WorkflowNavigator (next node routing)
```

The main execution loop in `WorkflowExecutionCoordinator.execute()`:

1. Get the current node from the graph via `WorkflowNavigator`
2. Skip already-completed nodes (for resume from checkpoint)
3. Register per-node timeout on the `TimeoutManager`
4. Execute the node via `NodeExecutionCoordinator.executeNode()`
5. Process the result: navigate to the next node, or handle failure
6. Apply workflow-level failure policies: `retry` / `continue` / `fail`
7. Support pause/resume/stop via interruption handling