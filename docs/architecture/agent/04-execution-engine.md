# Agent Execution Engine

## 1. Architecture: Coordinator-Executor-Handler Pattern

The agent execution engine follows a layered pattern with clear separation of concerns:

```
AgentLoopCoordinator            ← Lifecycle management (execute, pause, resume, stop)
  └─ AgentLoopExecutor          ← Stateless execution of a single agent loop
       └─ AgentExecutionCoordinator  ← Main iteration loop
            └─ AgentIterationCoordinator ← Per-iteration orchestration
                 ├─ LLMExecutionCoordinator  ← Shared LLM call execution
                 ├─ ToolExecutionCoordinator ← Tool call delegation
                 ├─ HookHandler              ← Before/after hooks
                 └─ TriggerHandler           ← Trigger execution
```

## 2. Coordinator Layer

### AgentLoopCoordinator

The top-level coordinator managing the full lifecycle of an agent loop execution:

- `execute(agentLoopId, config, options)` → Creates entity, executes, returns result
- `executeStream(agentLoopId, config, options)` → Streaming execution
- `pauseAgentLoop(agentLoopId)` → Pauses with optional checkpoint
- `resumeAgentLoop(agentLoopId, options?)` → Resumes from checkpoint
- `cancelAgentLoop(agentLoopId)` → Cancels with cleanup
- `continueAgentLoop(agentLoopId, options?)` → Continue with error context

Key responsibilities:
- Creates entity via `AgentLoopFactory`
- Delegates execution to `AgentLoopExecutor`
- Manages checkpoint creation at lifecycle boundaries
- Handles task registration for async execution (implements `AgentTaskManager`)

### AgentExecutionCoordinator

The main execution loop that processes iterations:

```
execute():
  1. Check interruption
  2. Execute state transition to RUNNING
  3. Loop iterations:
     while (iteration < maxIterations):
       a. Check retry budget
       b. Execute iteration via AgentIterationCoordinator
       c. Check termination conditions:
          - Final answer (no tool calls) → COMPLETED
          - maxIterations reached → COMPLETED
          - Interruption (PAUSE/STOP) → PAUSED/CANCELLED
          - Error → retry or FAILED
  4. Execute state transition to COMPLETED
  5. Return AgentLoopResult
```

**Retry Support**:
- Wraps iteration execution with exponential backoff retry
- Configurable retry policy (max retries, backoff strategy)
- Timeout protection with per-execution timeout
- Retry budget tracking (delay-only or total-time modes)

### AgentIterationCoordinator

Orchestrates a single iteration's execution:

```
executeIteration():
  1. Check interruption
  2. Execute BEFORE_ITERATION hooks
  3. Prepare messages (conversation context)
  4. Execute BEFORE_LLM_CALL hooks
  5. Call LLM via LLMExecutionCoordinator
  6. Execute AFTER_LLM_CALL hooks
  7. Process LLM response:
     a. If final answer: record iteration, check termination
     b. If tool calls: execute tools via ToolExecutionCoordinator
        - Execute BEFORE_TOOL_CALL hooks
        - Execute tool calls with approval
        - Execute AFTER_TOOL_CALL hooks
        - Add tool results to conversation
        - Continue loop for next iteration
     c. If both: process tool calls, then check for final answer
  8. Execute AFTER_ITERATION hooks
  9. Trigger execution (via executeAgentTriggers)
  10. Check for attempt completion
```

### AgentLoopStateTransitor

Handles atomic state transitions and lifecycle events:

```
startAgentLoop(entity)          → RUNNING
completeAgentLoop(entity)       → COMPLETED
failAgentLoop(entity, error)    → FAILED
pauseAgentLoop(entity)          → PAUSED
resumeAgentLoop(entity)         → RUNNING
cancelAgentLoop(entity)         → CANCELLED
```

Each transition:
1. Validates the transition is legal
2. Updates entity state via `AgentLoopState`
3. Emits appropriate lifecycle event

### ConversationCoordinator

Provides stateless conversation management coordination:

- `getConversationManager(agentLoopId)` → Get the conversation session
- `getNormalizedHistory(agentLoopId)` → Get normalized message history
- `getConversationStats(agentLoopId)` → Get conversation statistics

### ToolExecutionCoordinator

Coordinates tool call execution within an iteration:

```
executeTools(toolCalls, entity):
  1. For each tool call:
     a. Check approval (auto-approve or confirm)
     b. Execute BEFORE_TOOL_CALL hooks
     c. Execute tool via ToolCallExecutor
     d. Execute AFTER_TOOL_CALL hooks
     e. Add tool result to conversation
     f. Emit tool execution events
  2. Return tool execution results
```

## 3. Executor Layer

### AgentLoopExecutor

The stateless executor that serves as the execution entry point:

```
AgentLoopExecutor.execute(entity, config):
  1. Validate configuration
  2. Create AgentExecutionCoordinator
  3. Create LLMExecutionCoordinator, ToolExecutionCoordinator
  4. Prepare tool schemas
  5. Validate tool call protocol compatibility
  6. Delegate to AgentExecutionCoordinator.execute()
  7. Return AgentLoopResult
```

Design principles:
- Stateless: all state managed through AgentLoopEntity
- Supports pause/resume via interruption handling
- Consistent architecture with WorkflowExecutor
- No loop iteration logic (handled by coordinators)

## 4. Execution Flow Summary

```
1. AgentLoopCoordinator.execute()
   ├── AgentLoopFactory.create() → AgentLoopEntity
   ├── AgentLoopExecutor.execute(entity, config)
   │   ├── AgentExecutionCoordinator.execute(entity, ...)
   │   │   ├── AgentLoopStateTransitor.startAgentLoop()
   │   │   ├── Iteration loop:
   │   │   │   └── AgentIterationCoordinator.executeIteration(entity, ...)
   │   │   │       ├── Hooks (BEFORE_ITERATION)
   │   │   │       ├── Hooks (BEFORE_LLM_CALL)
   │   │   │       ├── LLMExecutionCoordinator.execute()
   │   │   │       ├── Hooks (AFTER_LLM_CALL)
   │   │   │       ├── ToolExecutionCoordinator.executeTools()
   │   │   │       │   ├── Hooks (BEFORE_TOOL_CALL, AFTER_TOOL_CALL)
   │   │   │       │   └── ToolCallExecutor.execute()
   │   │   │       ├── Hooks (AFTER_ITERATION)
   │   │   │       └── executeAgentTriggers()
   │   │   └── AgentLoopStateTransitor.completeAgentLoop()
   │   └── Return AgentLoopResult
   └── Return result
```

## 5. Key Design Patterns

### Execution Layers

```
AgentLoopCoordinator (lifecycle)
  └─ AgentLoopExecutor (entry point, stateless)
       └─ AgentExecutionCoordinator (main loop)
            └─ AgentIterationCoordinator (per-iteration)
```

### Dependencies

- Coordinators are stateless (DI-injected)
- Executors are instantiated per-execution
- Handlers are pure functions
- State is managed through AgentLoopEntity

### Interruption Integration

- Every iteration checks interruption state
- AbortController provides cancellation signals
- Graceful pause: iteration completes, then pauses
- Checkpoint created on pause for resume support