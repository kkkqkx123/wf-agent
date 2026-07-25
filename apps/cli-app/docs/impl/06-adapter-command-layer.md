# Adapter & Command Layer

## Overview

The adapter and command layers provide the bridge between CLI commands and the SDK, handling business logic and data transformation.

## Adapter Layer

### Base Adapter (`src/adapters/base-adapter.ts`)

Extends `BaseAppAdapter` from `@wf-agent/runtime/adapters`:
- Retrieves SDK instance from `getSDKInstance()` singleton
- Implements `executeWithErrorHandling` pattern
- Converts SDK errors to `CLIError`

### Domain Adapters (36 files)

Each adapter wraps SDK calls for a specific domain:

| Adapter | File | Key Operations |
|---------|------|----------------|
| `WorkflowAdapter` | `workflow-adapter.ts` | CRUD: list, get, delete, register |
| `WorkflowExecutionAdapter` | `workflow-execution-adapter.ts` | run, list, get, cancel, progress |
| `WorkflowGraphAdapter` | `workflow-graph-adapter.ts` | getNodes, getEdges, structure |
| `AgentLoopAdapter` | `agent-loop-adapter.ts` | start, list, get, cancel, listRunning |
| `AgentProfileAdapter` | `agent-profile-adapter.ts` | CRUD for agent profiles |
| `CheckpointAdapter` | `checkpoint-adapter.ts` | create, restore, list, delete |
| `TemplateAdapter` | `template-adapter.ts` | register, list, get, delete |
| `LLMProfileAdapter` | `llm-profile-adapter.ts` | CRUD for LLM configurations |
| `ScriptAdapter` | `script-adapter.ts` | register, list, get, delete |
| `ToolAdapter` | `tool-adapter.ts` | register, list, get, delete |
| `PluginAdapter` | `plugin-adapter.ts` | install, uninstall, enable, disable, list |
| `TriggerAdapter` | `trigger-adapter.ts` | register, list, get, delete |
| `MessageAdapter` | `message-adapter.ts` | send, list, get |
| `VariableAdapter` | `variable-adapter.ts` | set, get, list, delete |
| `EventAdapter` | `event-adapter.ts` | list, get, subscribe |
| `SkillAdapter` | `skill-adapter.ts` | register, list, get, delete |
| `SearchAdapter` | `search-adapter.ts` | full-text search |
| `MetricsAdapter` | `metrics-adapter.ts` | show, aggregate |
| `ProgressTrackingAdapter` | `progress-tracking-adapter.ts` | progress queries |
| `StorageDiagnosticsAdapter` | `storage-diagnostics-adapter.ts` | diagnostics, cleanup |
| `HookAdapter` | `hook-adapter.ts` | register, list |
| `ApprovalAdapter` | `approval-adapter.ts` | list, approve, reject |
| `UserInteractionAdapter` | `user-interaction-adapter.ts` | list, get |
| `MCPAdapter` | `mcp-adapter.ts` | MCP protocol operations |
| `SandboxAdapter` | `sandbox-adapter.ts` | create, manage |
| `ExecutionComparisonAdapter` | `execution-comparison-adapter.ts` | diff executions |
| `IterationAnalysisAdapter` | `iteration-analysis-adapter.ts` | iteration inspection |
| `AgentPerformanceAnalysisAdapter` | `agent-performance-analysis-adapter.ts` | performance metrics |
| `AgentErrorAnalysisAdapter` | `agent-error-analysis-adapter.ts` | error analysis |
| `WorkflowErrorAnalysisAdapter` | `workflow-error-analysis-adapter.ts` | workflow error analysis |

## Command Layer

### Command Structure (28 domain directories in `src/commands/`)

Each domain has a `create*Commands()` function returning a Commander `Command` group.

| Directory | Domain | Subcommands |
|-----------|--------|-------------|
| `workflow/` | Workflow CRUD | register, register-batch, list, get, delete, update, error |
| `workflow-execution/` | Execution lifecycle | run, list, get, cancel, compare, progress |
| `workflow-graph/` | Graph visualization | nodes, edges, structure |
| `workflow-version/` | Version management | list, rollback, diff |
| `checkpoint/` | Checkpoints | create, restore, list, delete |
| `template/` | Templates | register, list, get, delete |
| `llm-profile/` | LLM Profiles | CRUD |
| `script/` | Scripts | register, list, get, delete |
| `tool/` | Tools | register, list, get, delete |
| `plugin/` | Plugins | install, uninstall, enable, disable, list |
| `trigger/` | Triggers | register, list, get, delete |
| `message/` | Messages | send, list, get |
| `variable/` | Variables | set, get, list, delete |
| `event/` | Events | list, get, subscribe |
| `agent/` | Agent Loops | start, list, get, perf, error, template |
| `agent-profile/` | Agent Profiles | CRUD |
| `agent-loop-iteration/` | Iteration inspection | get, list |
| `skill/` | Skills | register, list, get, delete |
| `metrics/` | Metrics | show, aggregate |
| `storage/` | Storage | diagnostics, cleanup |
| `search/` | Search | full-text |
| `task/` | Tasks | list, get, stats |
| `hook/` | Hook Templates | register, list |
| `predefined/` | Predefined Resources | list |
| `user-interaction/` | User Interactions | list, get |
| `query/` | Queries | flexible query |
| `mcp/` | MCP Integration | MCP operations |
| `sandbox/` | Sandboxes | create, manage |
| `approval/` | Approvals | list, approve, reject |
| `progress/` | Progress | show |

### Command Pattern

```typescript
export function createWorkflowCommands(): Command {
  const cmd = new Command("workflow");
  cmd.command("list").action(async () => {
    const adapter = new WorkflowAdapter();
    const workflows = await adapter.listWorkflows();
    // format and output
  });
  // ...
  return cmd;
}
```

## Service Layer

### `ExecutionService` (`src/services/execution/execution-service.ts`)

Unified workflow execution via SDK:
- Blocking mode — waits for completion
- Foreground mode — uses pseudo-terminal (node-pty)
- Background mode — uses child_process fork with IPC

### `TerminalManager` (`src/services/terminal/terminal-manager.ts`)

Manages pseudo-terminal sessions:
- node-pty for foreground execution
- child_process for background execution
- Lifecycle management, I/O forwarding, event emission

### `CLIUserInteractionManager` (`src/handlers/user-interaction/`)

Handles user interaction during execution:
- `FollowUpQuestionHandler` — follow-up question prompts
- `ToolApprovalHandler` — tool approval flow

## Data Flow

```
CLI Command
    ↓
Adapter (executeWithErrorHandling)
    ↓
SDK (getSDKInstance)
    ↓
Storage Layer
    ↓
Formatter → CLIOutput → stdout
```

For TUI mode:
```
TUI Screen
    ↓
Adapter (direct call)
    ↓
SDK
    ↓
Screen state update → render()
```
