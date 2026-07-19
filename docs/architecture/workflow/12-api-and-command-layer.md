# API and Command Layer

## 1. Architecture

The API layer provides a unified command-based interface for interacting with the workflow engine. It follows the **Command pattern** with a clear separation of concerns.

```
API Layer
├── Commands (Execution operations)
├── Resources (CRUD operations)
├── Builders (Workflow definition)
└── Validators (Parameter validation)
```

## 2. Command Pattern

### Command Types

| Command Category | Commands |
|-----------------|----------|
| **Workflow Execution** | `ExecuteWorkflowCommand`, `ExecuteWorkflowStreamCommand` |
| **Workflow Lifecycle** | `PauseWorkflowCommand`, `ResumeWorkflowCommand`, `CancelWorkflowCommand` |
| **Checkpoint** | `CreateCheckpointCommand`, `RestoreFromCheckpointCommand` |
| **Trigger** | `EnableTriggerCommand`, `DisableTriggerCommand` |
| **Agent** | `RunAgentLoopCommand`, `RunAgentLoopStreamCommand`, `CancelAgentLoopCommand`, `PauseAgentLoopCommand`, `ResumeAgentLoopCommand` |
| **Shared** | `GenerateCommand`, `ExecuteToolCommand`, `ExecuteScriptCommand`, `DispatchEventCommand` |

### Command Structure

```typescript
abstract class BaseCommand<TParams, TResult> {
  abstract execute(): Promise<TResult>;
  validate(): CommandValidationResult;
  getMetadata(): CommandMetadata;
}
```

### Command Execution Flow

```
SDK.executeCommand(command)
  ├── Validate command parameters
  ├── Dispatch to WorkflowLifecycleCoordinator
  ├── Handle execution
  └── Return ExecutionResult
```

## 3. Resource APIs

### Workflow Resources

| Resource API | Operations |
|-------------|-----------|
| `WorkflowRegistryAPI` | CRUD for workflow templates |
| `WorkflowExecutionRegistryAPI` | Query execution instances |
| `NodeRegistryAPI` | Node template management |
| `TriggerTemplateRegistryAPI` | Trigger template management |
| `CheckpointResourceAPI` | Checkpoint query and management |
| `MessageResourceAPI` | Message history query |
| `VariableResourceAPI` | Variable inspection |
| `UserInteractionResourceAPI` | User interaction management |
| `WorkflowGraphQueryAPI` | Graph structure query |

### Shared Resources

| Resource API | Operations |
|-------------|-----------|
| `ToolRegistryAPI` | Tool registration and query |
| `ScriptRegistryAPI` | Script management |
| `LLMProfileRegistryAPI` | LLM profile management |
| `SkillRegistryAPI` | Skill management |
| `EventResourceAPI` | Event history query |
| `MetricsResourceAPI` | Metrics query and export |
| `TaskResourceAPI` | Task management |
| `StorageDiagnosticsAPI` | Storage health check |
| `SearchAPI` | Cross-resource search |

## 4. Validation

### Command Validators

| Validator | Validates |
|-----------|-----------|
| `validateWorkflowExecutionParams` | ExecuteWorkflow parameters |
| `validateWorkflowLifecycleParams` | Pause/Resume/Stop parameters |
| `validateCheckpointCreationParams` | Checkpoint creation parameters |
| `validateCheckpointRestorationParams` | Checkpoint restoration parameters |
| `validateTriggerParams` | Trigger enable/disable parameters |
| `validateAgentLoopRunParams` | Agent loop execution parameters |
| `validateAgentLoopControlParams` | Agent lifecycle parameters |

## 5. Builders

### Workflow Builders

| Builder | Purpose |
|---------|---------|
| `WorkflowBuilder` | Build workflow templates |
| `ExecutionBuilder` | Build execution configurations |
| `NodeTemplateBuilder` | Build node template definitions |
| `TriggerTemplateBuilder` | Build trigger template definitions |

### Agent Builders

| Builder | Purpose |
|---------|---------|
| `AgentLoopConfigBuilder` | Build agent loop configurations |
| `AgentDefinitionBuilder` | Build agent definitions |
| `AgentToolConfigBuilder` | Build tool configurations |
| `AgentHookBuilder` | Build hook configurations |
| `AgentTriggerBuilder` | Build trigger configurations |