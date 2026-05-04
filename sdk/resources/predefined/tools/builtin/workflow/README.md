# Workflow Builtin Tools

This directory contains built-in tools that enable agents to interact with workflow executions. These tools provide a unified interface for triggering, monitoring, and managing workflows from within agent loops or other execution contexts.

## Overview

The workflow builtin tools allow LLM-powered agents to:
- **Execute workflows** dynamically with custom inputs
- **Monitor execution status** of asynchronous workflow tasks
- **Cancel running workflows** when needed

These tools are automatically registered in the builtin tool registry and made available to agents based on their configuration.

## Available Tools

### 1. `execute_workflow`

Executes a predefined workflow with the given input parameters.

**Parameters:**
- `workflowId` (string, required): The ID of the workflow to execute
- `input` (object, optional): Input parameters to pass to the workflow
- `waitForCompletion` (boolean, optional, default: `true`): Whether to wait for the workflow to complete
- `timeout` (number, optional): Timeout in milliseconds

**Return Value:**
- When `waitForCompletion=true`:
  ```typescript
  {
    success: true,
    status: "completed",
    output: Record<string, unknown>,
    executionTime: number
  }
  ```
- When `waitForCompletion=false`:
  ```typescript
  {
    success: true,
    status: "submitted",
    taskId: string,
    message: string
  }
  ```

**Usage Example:**
```typescript
// Synchronous execution
const result = await execute_workflow({
  workflowId: "data-processing-workflow",
  input: { dataset: "sales_2024", format: "csv" },
  waitForCompletion: true
});

// Asynchronous execution
const submission = await execute_workflow({
  workflowId: "long-running-analysis",
  input: { model: "forecast-v2" },
  waitForCompletion: false
});
// Use submission.taskId to query status later
```

**Best Practices:**
- Use `waitForCompletion=false` for long-running workflows to avoid blocking the agent
- Provide clear and structured input data matching the workflow's expected schema
- Handle both success and error cases in your agent logic

---

### 2. `query_workflow_status`

Queries the status of an asynchronously submitted workflow task.

**Parameters:**
- `taskId` (string, required): The task ID returned from an async `execute_workflow` call

**Return Value:**
```typescript
{
  success: boolean,
  status: "pending" | "running" | "completed" | "failed" | "cancelled" | "not_found",
  executionId?: string,
  workflowId?: string,
  createdAt?: number,
  updatedAt?: number,
  message?: string
}
```

**Usage Example:**
```typescript
const status = await query_workflow_status({
  taskId: "task-abc123"
});

if (status.status === "completed") {
  console.log("Workflow finished successfully");
} else if (status.status === "failed") {
  console.error("Workflow failed");
}
```

**Best Practices:**
- Poll periodically for async workflows (e.g., every 5-10 seconds)
- Implement exponential backoff to avoid excessive API calls
- Check for terminal states (`completed`, `failed`, `cancelled`) before stopping polling

---

### 3. `cancel_workflow`

Cancels a running workflow task.

**Parameters:**
- `taskId` (string, required): The task ID to cancel

**Return Value:**
```typescript
{
  success: boolean,
  taskId: string,
  message: string
}
```

**Usage Example:**
```typescript
const result = await cancel_workflow({
  taskId: "task-abc123"
});

if (result.success) {
  console.log("Workflow cancelled successfully");
}
```

**Best Practices:**
- Only async workflows (submitted with `waitForCompletion=false`) can be cancelled
- Verify the task is in a cancellable state before attempting cancellation
- Handle partial execution results gracefully after cancellation

---

## Architecture

### Tool Categories

Each builtin tool includes an optional `category` field for internal organization:

- **`workflow`**: Tools that execute, monitor, or manage workflow instances
- **`agent`**: Tools that invoke or manage agent loops and subagents
- **`filesystem`**: File reading, writing, and manipulation tools
- **`shell`**: Command execution and terminal interaction tools
- **`memory`**: Session notes and context management tools
- **`code`**: Code analysis, editing, and generation tools
- **`http`**: REST API and web service integration tools
- **`integration`**: Third-party service integrations (MCP, external APIs)

**Important:** The category field is used for:
- Internal tool registry organization
- UI grouping in settings panels
- Programmatic filtering (`getByCategory()`)
- Developer experience and debugging

The category is **NOT included in tool descriptions sent to LLMs**. It's purely for internal organization and has no impact on agent behavior.

### Execution Hierarchy

All workflow tools maintain proper parent-child relationships through the `ExecutionHierarchyRegistry`:

```
Agent Loop / Parent Workflow
  └─ execute_workflow (tool invocation)
      └─ Child Workflow Execution
          ├─ Node 1
          ├─ Node 2
          └─ ...
```

**Key Features:**
- **Parent Context Propagation:** Child workflows inherit the parent's execution context
- **Lifecycle Linkage:** If the parent is cancelled, all child workflows receive an abort signal
- **Traceability:** Each workflow execution includes a unique `triggerId` linking it back to the tool invocation

### Dependency Injection

The handlers retrieve services from the DI container:
- `TriggeredSubworkflowHandler`: Manages workflow execution lifecycle
- `EventRegistry`: (Future) For emitting observability events

### Error Handling

All tools use `RuntimeValidationError` for parameter validation with detailed context:
- Operation name
- Field name
- Current execution ID
- Parent execution availability

---

## Common Patterns

### Pattern 1: Fire-and-Forget with Status Polling

```typescript
// Submit workflow asynchronously
const submission = await execute_workflow({
  workflowId: "background-task",
  waitForCompletion: false
});

// Poll for completion
let status;
do {
  await sleep(5000); // Wait 5 seconds
  status = await query_workflow_status({ taskId: submission.taskId });
} while (status.status === "pending" || status.status === "running");

if (status.status === "completed") {
  // Handle success
} else {
  // Handle failure
}
```

### Pattern 2: Conditional Cancellation

```typescript
const submission = await execute_workflow({
  workflowId: "risky-operation",
  waitForCompletion: false
});

// Monitor and cancel if needed
const status = await query_workflow_status({ taskId: submission.taskId });
if (shouldCancel(status)) {
  await cancel_workflow({ taskId: submission.taskId });
}
```

### Pattern 3: Nested Workflow Orchestration

```typescript
// Agent can orchestrate multiple workflows
await execute_workflow({
  workflowId: "data-extraction",
  input: { source: "api" }
});

await execute_workflow({
  workflowId: "data-transformation",
  input: { format: "json" }
});

await execute_workflow({
  workflowId: "data-loading",
  input: { target: "database" }
});
```

---

## Testing

Tests are located in `__tests__/workflow-handlers.test.ts`.

**Run Tests:**
```bash
cd sdk
pnpm test resources/predefined/tools/builtin/workflow/__tests__
```

**Test Coverage:**
- Parameter validation
- Synchronous execution
- Asynchronous submission
- Status querying
- Task cancellation
- Error handling

---

## Future Enhancements

Potential improvements under consideration:

1. **Event Emission:** Add lifecycle events (started, completed, failed) for better observability
2. **Retry Logic:** Implement automatic retry with exponential backoff for transient failures
3. **Batch Execution:** Support executing multiple workflows in parallel
4. **Workflow Discovery:** Add a `list_workflows` tool for dynamic workflow discovery
5. **Input Validation:** Pre-validate workflow inputs against workflow schema before execution

---

## Related Documentation

- [Builtin Subagent and Workflow Tools Design](../../../../docs/resources/tools/builtin-subagent-workflow-tools.md)
- [Agent Loop Architecture](../../../../docs/architecture/agent-loop-architecture.md)
- [Workflow Execution Types](../../../../workflow/execution/types/workflow-tool.types.ts)
- [Triggered Subworkflow Handler](../../../../workflow/execution/handlers/triggered-subworkflow-handler.ts)
