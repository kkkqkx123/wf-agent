# Builtin Subagent and Workflow Tools Design

## 1. Overview
This document outlines the design for extending the `sdk/resources/predefined/tools/builtin` directory to include tools for invoking subagents and workflows. The goal is to empower Agent Loops with the ability to autonomously orchestrate complex tasks by delegating to specialized subagents or predefined workflows.

## 2. Motivation
- **Autonomy:** Allows the main agent to act as a orchestrator, breaking down complex goals into smaller, manageable sub-tasks.
- **Modularity:** Encapsulates specific domain logic (e.g., code review, deployment) into reusable subagents or workflows.
- **Consistency:** Aligns with the existing `execute_workflow` tool pattern, providing a unified interface for all execution units.

## 3. Proposed Tools

### 3.1 `call_agent`
A synchronous tool that starts a predefined subagent and waits for its completion.

**Parameters:**
- `agentProfileId` (string, required): The ID of the subagent profile to execute.
- `prompt` (string, required): The task description or prompt for the subagent.
- `input` (object, optional): Additional context or variables to pass to the subagent.
- `waitForCompletion` (boolean, optional): Defaults to `true`. If `false`, returns immediately with a task ID.

**Behavior:**
- Creates an `AgentLoopEntity` via `AgentLoopFactory`.
- Establishes a parent-child relationship using `ExecutionHierarchyRegistry`.
- Inherits or overrides tool visibility based on the subagent's profile configuration.

### 3.2 `execute_workflow` (Existing)
Currently implemented in `builtin/workflow/execute-workflow`. It allows agents to trigger graph-based workflows.

**Enhancements:**
- Ensure consistent error handling and status reporting compared to `call_agent`.
- Support for passing complex variable scopes from the parent agent to the child workflow.

## 4. Architecture Integration

### 4.1 Execution Hierarchy
Both tools must integrate with the `ExecutionHierarchyRegistry`:
- **Parent Context:** The current `WorkflowExecution` or `AgentLoop` becomes the parent.
- **Lifecycle Linkage:** If the parent is cancelled, all child executions (agents or workflows) should receive an abort signal.

### 4.2 Dependency Injection
Handlers will retrieve necessary services from the DI container:
- `AgentLoopCoordinator`: For managing subagent lifecycles.
- `TriggeredSubworkflowHandler`: For managing workflow executions.
- `EventRegistry`: For emitting start/completion events.

## 5. Implementation Plan
1. **Directory Structure:** Create `sdk/resources/predefined/tools/builtin/agent/`.
2. **Schema & Description:** Define Zod schemas and natural language descriptions for LLM prompting.
3. **Handler Logic:** Implement the execution logic, ensuring proper context propagation.
4. **Registry Update:** Add the new tools to `builtin/registry.ts`.

## 6. Reference
- **Lim-Code:** `ref/Lim-Code-1.0.93/backend/tools/subagents/executor.ts` for subagent execution patterns.
- **Current SDK:** `sdk/workflow/execution/handlers/node-handlers/agent-loop-handler.ts` for existing integration points.
