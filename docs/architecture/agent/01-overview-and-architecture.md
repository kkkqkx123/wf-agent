# Agent Architecture Overview

## 1. Introduction

The Agent module provides a loop-based execution engine for LLM-powered agents within the Modular Agent Framework. Unlike the workflow engine's DAG-based execution, the agent loop follows a conversational iteration pattern: each iteration calls the LLM, processes the response (final answer or tool calls), and repeats until a termination condition is met.

### Architecture Layers

```
┌─────────────────────────────────────────────────────┐
│                    SDK Kit Layer                       │
│  (SDKKit, AgentDefinitionBuilder, ExecutionAPI)       │
├─────────────────────────────────────────────────────┤
│                  SDK Core Layer                        │
│  ┌─────────────────────────────────────────────────┐ │
│  │          API Layer (Commands, Resources)          │ │
│  ├─────────────────────────────────────────────────┤ │
│  │           Agent Loop Execution Engine             │ │
│  │  (Coordinators, Executors, Handlers, State        │ │
│  │   Managers, Checkpoint, Registry)                  │ │
│  ├─────────────────────────────────────────────────┤ │
│  │    Shared Services (LLM, Tools, Events, Hooks)    │ │
│  ├─────────────────────────────────────────────────┤ │
│  │   DI Container & Plugin System                    │ │
│  └─────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────┤
│                Type System (types package)             │
│  (AgentLoopConfig, AgentLoopState, Execution types)   │
└─────────────────────────────────────────────────────┘
```

## 2. Module Boundaries

### packages/sdk/agent/ — Core Agent Engine

Sub-modules:

| Module | Responsibility |
|--------|---------------|
| `entities/` | Core data entity: `AgentLoopEntity` |
| `state-managers/` | State management: `AgentLoopState`, `AgentStateCoordinator` |
| `execution/coordinators/` | Coordination layer: `AgentLoopCoordinator`, `AgentExecutionCoordinator`, `AgentIterationCoordinator`, `ConversationCoordinator`, `ToolExecutionCoordinator`, `AgentLoopStateTransitor`, `TriggeredAgentExecutionManager` |
| `execution/executors/` | `AgentLoopExecutor` — stateless execution entry point |
| `execution/factories/` | `AgentLoopFactory` — entity creation and checkpoint restoration |
| `execution/handlers/` | Error handler, hook handlers, trigger handlers, lifecycle functions |
| `execution/types/` | Agent interruption types |
| `execution/utils/` | Agent interruption utilities |
| `checkpoint/` | Checkpoint coordinator, state manager, policy, config resolver |
| `registry/` | `AgentLoopRegistry`, `IAgentExecutionRegistry` |
| `validation/` | Config validation, protocol validation |

### packages/sdk/api/agent/ — API Layer

| Module | Responsibility |
|--------|---------------|
| `builders/` | `AgentDefinitionBuilder`, `AgentLoopConfigBuilder`, `AgentHookBuilder`, `AgentToolConfigBuilder`, `AgentTriggerBuilder` |
| `operations/` | Commands: `RunAgentLoopCommand`, `RunAgentLoopStreamCommand`, `PauseAgentLoopCommand`, `ResumeAgentLoopCommand`, `CancelAgentLoopCommand`, checkpoint commands, trigger commands |
| `resources/` | Resource APIs: execution state, registry, checkpoint, variables, messages, hooks, triggers, templates, error analysis |

### packages/sdk/agent/ — Dependencies on Shared Modules

The agent module heavily relies on shared infrastructure:

| Shared Module | Usage |
|---------------|-------|
| `shared/checkpoint/` | Base checkpoint coordinator, state manager, hierarchy restore |
| `shared/events/` | Event registry, event builders, execution event bus |
| `shared/messaging/` | ConversationSession, BaseStateCoordinator, message builders |
| `shared/coordinators/` | LLMExecutionCoordinator, ToolApprovalCoordinator, RetryBudget |
| `shared/registry/` | ExecutionStore, ToolRegistry, EventRegistry, TaskRegistry, ExecutionHierarchyRegistry |
| `shared/execution/` | ExecutionHierarchyManager, HierarchyIntegrityService |
| `shared/errors/` | ErrorChainManager |
| `shared/records/` | ExecutionRecordManager |
| `shared/protection/` | TimeoutManager, ToolFailureProtectionState |
| `shared/utils/interruption/` | InterruptionState, AbortController, interruption utilities |
| `shared/hooks/` | HookExecutor, hook types |
| `shared/tools/` | Tool schema helpers, tool declaration formatter |

## 3. Execution Model

The agent loop execution follows a **coordinator-executor-handler** pattern, layered with iteration-based execution:

```
AgentLoopCoordinator (lifecycle management)
  └─ AgentLoopExecutor (stateless, single execution entry)
       └─ AgentExecutionCoordinator (main loop: iterations)
            └─ AgentIterationCoordinator (per-iteration orchestration)
                 ├─ HookHandler (BEFORE_ITERATION, BEFORE_LLM_CALL, etc.)
                 ├─ LLMExecutionCoordinator (shared, LLM call)
                 ├─ HookHandler (AFTER_LLM_CALL, etc.)
                 ├─ ToolExecutionCoordinator (tool call delegation)
                 │   ├─ HookHandler (BEFORE_TOOL_CALL, AFTER_TOOL_CALL)
                 │   └─ ToolCallExecutor (actual tool execution)
                 └─ TriggerHandler (trigger execution after iteration)
```

### Iteration Loop

```
for each iteration:
  1. Check interruption (pause/stop)
  2. Execute BEFORE_ITERATION hooks
  3. Prepare messages (conversation context)
  4. Execute BEFORE_LLM_CALL hooks
  5. Call LLM via LLMExecutionCoordinator
  6. Execute AFTER_LLM_CALL hooks
  7. Process LLM response:
     a. If final answer → complete iteration, check termination
     b. If tool calls → execute tools via ToolExecutionCoordinator
        - Execute BEFORE_TOOL_CALL hooks
        - Execute tool calls (with approval)
        - Execute AFTER_TOOL_CALL hooks
        - Add tool results to conversation
     c. If both → process tool calls, then check for final answer
  8. Execute AFTER_ITERATION hooks
  9. Check termination conditions:
     - maxIterations reached
     - final answer received (no tool calls)
     - interruption (pause/stop)
     - error (retry or fail)
```

## 4. Key Design Decisions

### Loop vs DAG

Unlike workflow's DAG-based execution, the agent loop is iterative:
- **No fixed graph**: The agent dynamically decides next steps via LLM
- **Conversation-driven**: State is primarily the message history
- **Tool-augmented**: LLM can call tools, but the call graph is not predefined

### Separation of Config and State

- `AgentLoopRuntimeConfig`: Immutable configuration with callbacks (not serialized)
- `AgentLoopState`: Mutable execution state (serialized to checkpoints)
- `AgentLoopEntity`: Wrapper combining config + state + runtime managers

### Checkpoint Strategy

- Only `AgentLoopState` is serialized (iteration history, tool calls, streaming state)
- Config is NOT serialized (contains functions)
- On restore: config is re-provided, state is restored from checkpoint

### Streaming Support

- Both message-level streaming and agent-level streaming events
- `RunAgentLoopStreamCommand` for streaming execution
- Stream events include token chunks, tool call progress, iteration status

### Retry and Timeout

- Per-iteration retry with exponential backoff
- Per-execution timeout
- Global retry budget tracking
- Time budget modes: delay-only or total-time