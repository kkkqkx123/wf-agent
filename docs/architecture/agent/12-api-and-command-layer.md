# Agent API and Command Layer

## 1. Architecture

The API layer provides a unified command-based interface for interacting with the agent loop engine. It follows the **Command pattern** with a clear separation of concerns.

```
API Layer (packages/sdk/api/agent/)
├── Builders (Agent definition construction)
├── Operations (Execution commands)
├── Resources (CRUD operations for agent artifacts)
└── Validators (Parameter validation)
```

## 2. Command Pattern

### Command Types

| Command Category | Commands |
|-----------------|----------|
| **Agent Execution** | `RunAgentLoopCommand`, `RunAgentLoopStreamCommand` |
| **Agent Lifecycle** | `PauseAgentLoopCommand`, `ResumeAgentLoopCommand`, `CancelAgentLoopCommand` |
| **Checkpoint** | `CreateCheckpointCommand`, `RestoreCheckpointCommand` |
| **Trigger** | `EnableAgentTriggerCommand`, `DisableAgentTriggerCommand` |

### Command Structure

```typescript
abstract class ExecutionCommand<TResult> {
  abstract execute(): Promise<TResult>;
  validate(): CommandValidationResult;
  getMetadata(): CommandMetadataDefinition;
}
```

### RunAgentLoopCommand

```
RunAgentLoopCommand
├── Parameters:
│   ├── config: AgentLoopRuntimeConfig
│   ├── options?: AgentLoopEntityOptions
│   └── timeoutMs?: number
│
├── execute():
│   1. Validate parameters
│   2. Delegate to AgentLoopCoordinator.execute()
│   3. Return AgentLoopResult
│
└── validate():
    ├── Validate config is provided
    ├── Validate timeout is positive (if provided)
    └── Return validation result
```

### RunAgentLoopStreamCommand

```
RunAgentLoopStreamCommand
├── Parameters:
│   ├── config: AgentLoopRuntimeConfig
│   ├── options?: AgentLoopEntityOptions
│   └── timeoutMs?: number
│
├── execute():
│   1. Validate parameters
│   2. Delegate to AgentLoopCoordinator.executeStream()
│   3. Return async iterable of stream events
│
└── validate():
    └── Same as RunAgentLoopCommand
```

### Lifecycle Commands

| Command | Operation | Description |
|---------|-----------|-------------|
| `PauseAgentLoopCommand` | `pauseAgentLoop(agentLoopId)` | Pause execution with optional checkpoint |
| `ResumeAgentLoopCommand` | `resumeAgentLoop(agentLoopId, options?)` | Resume from checkpoint with error context |
| `CancelAgentLoopCommand` | `cancelAgentLoop(agentLoopId)` | Cancel execution with cleanup |

## 3. Resource APIs

### Execution Resources

| Resource API | Operations |
|-------------|-----------|
| `AgentLoopResourceAPI` | Query agent loop execution state |
| `AgentLoopIterationAPI` | Query iteration history |
| `AgentExecutionStateAPI` | Query current execution state |
| `AgentExecutionGraphQueryAPI` | Query execution graph data |
| `AgentVariableResourceAPI` | Variable inspection |
| `AgentMessageResourceAPI` | Message history query |

### Registry Resources

| Resource API | Operations |
|-------------|-----------|
| `AgentLoopRegistryAPI` | CRUD for agent loop registrations |
| `AgentTemplateRegistryAPI` | Agent template management |
| `AgentHookTemplateRegistryAPI` | Hook template management |
| `AgentTriggerTemplateRegistryAPI` | Trigger template management |

### Checkpoint Resources

| Resource API | Operations |
|-------------|-----------|
| `AgentCheckpointResourceAPI` | Checkpoint query and management |
| `AgentFileCheckpointResourceAPI` | File checkpoint management |

### Error Resources

| Resource API | Operations |
|-------------|-----------|
| `AgentErrorAnalysisAPI` | Error analysis and pattern detection |

### User Interaction Resources

| Resource API | Operations |
|-------------|-----------|
| `AgentUserInteractionResourceAPI` | User interaction management |
| `AgentPerformanceAnalysisAPI` | Performance metrics |

## 4. Fluent Builders

### AgentDefinitionBuilder

Builds static `AgentLoopDefinition` objects for config-based agent definitions:

```
AgentDefinitionBuilder.create()
  .id("my-agent")
  .name("My Agent")
  .profileId("gpt-4")
  .systemPrompt("You are a helpful assistant")
  .maxIterations(10)
  .availableTools({ include: ["search", "calculate"] })
  .hooks([...])
  .triggers([...])
  .build() → AgentLoopDefinition
```

### AgentLoopConfigBuilder

Builds runtime `AgentLoopRuntimeConfig` objects:

```
AgentLoopConfigBuilder.create()
  .profileId("gpt-4")
  .systemPrompt("You are a helpful assistant")
  .initialUserMessage("Hello")
  .maxIterations(10)
  .availableTools({ include: ["search"] })
  .stream(true)
  .createCheckpointOnEnd(true)
  .build() → AgentLoopRuntimeConfig
```

### Specialized Builders

| Builder | Purpose |
|---------|---------|
| `AgentHookBuilder` | Build hook configurations with condition and action |
| `AgentToolConfigBuilder` | Build tool access configuration (include/exclude) |
| `AgentTriggerBuilder` | Build trigger configurations |

## 5. Command Execution Flow

```
SDK.executeCommand(RunAgentLoopCommand)
  1. Validate command parameters
  2. Dispatch to AgentLoopCoordinator.execute()
  3. Handle execution:
     a. Create entity via AgentLoopFactory
     b. Execute via AgentLoopExecutor
     c. Handle interruptions
     d. Create checkpoint on completion/error
  4. Return AgentLoopResult
```

## 6. Resource API Design

Each resource API follows a consistent pattern:

```
ResourceAPI
├── get(id) → Promise<Resource>
├── list(filter?) → Promise<Resource[]>
├── create(data) → Promise<ID>
├── update(id, data) → Promise<void>
└── delete(id) → Promise<void>
```

Resource APIs are designed for:
- Querying execution state and history
- Managing templates and registrations
- Inspecting checkpoints and variables
- Analyzing errors and performance