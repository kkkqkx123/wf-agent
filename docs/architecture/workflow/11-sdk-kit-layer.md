# SDK Kit Layer

## 1. Overview

The SDK Kit (`@wf-agent/sdk-kit`) provides a higher-level API for interacting with the workflow engine. It wraps the core SDK with fluent builder patterns and Result-based error handling.

## 2. SDKKit Class

The main entry point that coordinates all sub-modules:

```
SDKKit
├── workflow() → WorkflowAPI (workflow definition)
├── execution() → ExecutionAPI (workflow execution)
├── query() → QueryAPI (execution queries)
├── resource() → ResourceAPI (resource management)
├── events() → EventManager (event subscription)
├── analysis() → ComparisonAnalysis, ProgressAnalysis
├── plugins() → PluginManager (plugin management)
├── getConfig() → SDKKitOptions
└── getExecuteCommand() → ExecuteWorkflowCommandConstructor
```

## 3. Workflow Builder (Kit Layer)

`WorkflowBuilder` provides a fluent API for programmatic workflow definition:

```
WorkflowBuilder
├── node(id, config) → Result<this, KitError>
├── edge(from, to, condition?) → Result<this, KitError>
├── name(name) → Result<this, KitError>
├── description(desc) → Result<this, KitError>
├── metadata(data) → Result<this, KitError>
└── build() → Result<WorkflowTemplate, KitError[]>
```

### Design Principles

- **All methods return `Result<T, KitError>`** — no exceptions
- **build() collects all validation errors** — returns all errors at once
- **Chainable** — `.node().edge().build()` pattern
- **Type-safe** — fully typed configurations

### Result Pattern

```typescript
type Result<T, E> = Ok<T, E> | Err<T, E>;

// Chainable error handling:
builder
  .node("start", { type: "START" })
  .andThen(b => b.node("llm", { type: "LLM", config: { ... } }))
  .andThen(b => b.edge("start", "llm"))
  .andThen(b => b.build())
  .match(
    template => { /* success */ },
    errors => { /* handle errors */ }
  );
```

## 4. Execution Runner (Kit Layer)

`ExecutionRunner` provides simplified workflow execution:

```
ExecutionRunner
├── executeWorkflow(workflowId, input?, options?) → Promise<Result<ExecutionResult, KitError>>
└── onEvent(event, handler) → Register event listener

ExecutionBuilder (fluent)
├── input(data) → this
├── onProgress(handler) → this
├── onError(handler) → this
└── execute() → Promise<Result<ExecutionResult, KitError>>
```

## 5. Query Executor (Kit Layer)

`QueryExecutor` provides execution history queries:

```
QueryExecutor
├── getExecution(executionId) → Result<ExecutionRecord, KitError>
├── listExecutions(filter?) → Result<ExecutionRecord[], KitError>
└── getExecutionEvents(executionId, filter?) → Result<ExecutionEvent[], KitError>
```

## 6. Error Conversion

`ErrorConverter` translates SDK errors to `KitError`:

```
ErrorConverter
├── convertResult<T>(result) → Result<T, KitError>
└── toKitError(error) → KitError

KitError
├── message: string
├── code: KitErrorCode
└── details?: Record<string, unknown>
```

## 7. Plugin Manager (Kit Layer)

`PluginManager` manages SDK kit plugins:

```
PluginManager
├── register(plugin) → void
├── unregister(pluginId) → void
├── getPlugins() → Plugin[]
└── executeHook(hookName, context) → void
```

## 8. Resource Manager (Kit Layer)

`ResourceManager` provides access to shared resources:

```
ResourceManager
├── getWorkflowRegistry() → WorkflowRegistryAPI
├── getExecutionRegistry() → WorkflowExecutionRegistryAPI
├── getNodeRegistry() → NodeRegistryAPI
├── getTriggerRegistry() → TriggerTemplateRegistryAPI
└── getCheckpointRegistry() -> CheckpointResourceAPI
```