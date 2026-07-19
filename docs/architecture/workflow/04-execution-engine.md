# Workflow Execution Engine

## 1. Architecture: Coordinator-Executor-Handler Pattern

The execution engine follows a layered pattern with clear separation of concerns:

```
WorkflowLifecycleCoordinator     ← Lifecycle management (execute, pause, resume, stop)
  └─ WorkflowExecutor            ← Stateless execution of a single entity
       └─ WorkflowExecutionCoordinator  ← Main execution loop
            └─ NodeExecutionCoordinator ← Per-node orchestration
                 ├─ getNodeHandler()    ← Route to node-specific handler
                 ├─ NodeHandlerContextFactory ← Build handler context
                 ├─ HookHandler         ← Before/after node hooks
                 ├─ NodeCheckpointStrategy ← Checkpoint creation
                 └─ WorkflowNavigator   ← Next node routing
```

## 2. Coordinator Layer

### WorkflowLifecycleCoordinator

The top-level coordinator managing the full lifecycle of a workflow execution:

- `execute(workflowId, input)` → Creates entity, executes, returns result
- `pauseWorkflowExecution(executionId)` → Pauses with optional checkpoint
- `resumeWorkflowExecution(executionId)` → Resumes from checkpoint
- `stopWorkflowExecution(executionId)` → Cancels with cleanup
- `forceSetWorkflowExecutionStatus()` → Force status override for recovery

### WorkflowExecutionCoordinator

The main execution loop that processes nodes sequentially:

```
execute():
  for each attempt (retry loop):
    while (currentNodeId exists):
      1. Skip already-completed nodes (for resume)
      2. Register per-node timeout
      3. Execute node via NodeExecutionCoordinator
      4. Navigate to next node based on result
      5. Handle failure: retry | continue | fail
```

**Failure Policies**:
- `retry` — Retry the entire workflow from the beginning
- `continue` — Mark failed node as SKIPPED and continue
- `fail` — Stop execution and mark workflow as FAILED

### NodeExecutionCoordinator

Orchestrates a single node's execution:

```
executeNode(executionEntity, node, options):
  1. Create NodeHandlerContext (via NodeHandlerContextFactory)
  2. Execute BEFORE_EXECUTE hooks
  3. Create BEFORE_EXECUTE checkpoint (if configured)
  4. Execute node handler
  5. Create AFTER_EXECUTE checkpoint (if configured)
  6. Execute AFTER_EXECUTE hooks
  7. Handle interruption (pause/stop)
  8. Return NodeExecutionResult
```

### WorkflowStateTransitor

Handles atomic state transitions and cascade operations:

- `startWorkflowExecution()` → Initialize execution state
- `pauseWorkflowExecution()` → Pause with interruption records
- `resumeWorkflowExecution()` → Resume from pause
- `completeWorkflowExecution()` → Finalize with output
- `failWorkflowExecution()` → Fail with error chain
- `cancelWorkflowExecution()` → Cancel with cascade to children
- `cascadeCancel()` → Recursive cancellation of child executions
- `waitForChildExecutionsCompletion()` → Wait for fork branches

## 3. Executor Layer

### WorkflowExecutor

A **stateless** executor that focuses on executing a single `WorkflowExecutionEntity`:

- Stateless: All state is in `WorkflowExecutionEntity`
- Delegates to `WorkflowExecutionCoordinator` via factory
- Verifies graph existence before execution
- Lightweight, no DI container dependency

### WorkflowExecutionPool

A **global singleton** pool managing `WorkflowExecutor` instances:

- Dynamic scaling: Creates executors on demand
- Idle reclamation: Reclaims executors after timeout
- Configurable min/max pool size
- Used by triggered sub-workflows and dynamic executions

## 4. Factory Layer

### WorkflowExecutionBuilder

Builds `WorkflowExecutionEntity` instances from templates or graphs:

- `build(workflowGraph, input)` → Creates entity from preprocessed graph
- `buildFromTemplate(templateId)` → Loads template, preprocesses, builds
- `createCopy(existingEntity)` → Deep copy for retry
- `createChildExecution(config, parent)` → Creates child for SUBGRAPH/FORK/TRIGGERED

### NodeHandlerContextFactory

Creates context objects for each node handler type:

- `createHandlerContext(executionEntity, node, runtimeNode)` → Dispatches to type-specific factory
- `createLLMContext()` → `LLMHandlerContext` (conversation, tools, LLM config)
- `createForkContext()` → `ForkHandlerContext` (new execution builder, graph registry)
- `createSubgraphContext()` → `SubgraphHandlerContext` (child execution builder)
- `createAgentLoopContext()` → `AgentLoopHandlerContext` (agent loop dependencies)
- `createUserInteractionContext()` → `UserInteractionHandlerContext`
- `createToolVisibilityContext()` → `ToolVisibilityHandlerContext`
- `createContextProcessorContext()` → `ContextProcessorHandlerContext`

## 5. Trigger System

### TriggerCoordinator

Manages workflow triggers at runtime:

- `register(workflowTrigger, workflowId)` → Register trigger with condition matching
- `enable(triggerId)` / `disable(triggerId)` → Toggle trigger state
- `handleEvent(event)` → Match event against registered triggers, execute matched actions
- `executeTrigger()` → Execute trigger action (script, sub-workflow, etc.)

### Trigger Handler Types

| Handler | Action |
|---------|--------|
| `execute-script-handler` | Execute a script as trigger action |
| `execute-triggered-subworkflow-handler` | Execute a sub-workflow |
| `set-variable-handler` | Set a variable |
| `pause-execution-handler` | Pause execution |
| `resume-execution-handler` | Resume execution |
| `stop-execution-handler` | Stop execution |
| `skip-node-handler` | Skip a node |
| `send-notification-handler` | Send notification |
| `apply-message-operation-handler` | Apply message operations |