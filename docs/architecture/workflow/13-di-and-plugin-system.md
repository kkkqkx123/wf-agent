# DI Container and Plugin System

## 1. Dependency Injection Container

The DI container is built on `@wf-agent/common-utils` and provides centralized service registration and resolution.

### Container Configuration

`configureContainerBindings()` in `di/container-config.ts` registers all services:

```
Container Registrations:
├── Registries: WorkflowGraphRegistry, WorkflowExecutionRegistry, WorkflowRegistry,
│               EventRegistry, ToolRegistry, ScriptRegistry, NodeTemplateRegistry,
│               HookTemplateRegistry, TriggerTemplateRegistry, TaskRegistry,
│               ExecutionHierarchyRegistry, AgentLoopRegistry, ...
├── Coordinators: WorkflowLifecycleCoordinator, WorkflowStateTransitor,
│                 NodeExecutionCoordinator, TriggerCoordinator, VariableCoordinator,
│                 LLMExecutionCoordinator, ...
├── Executors: WorkflowExecutor, WorkflowExecutionPool, AgentLoopExecutor, ...
├── Services: LLMWrapper, LLMExecutor, ToolCallExecutor, ScriptExecutionService,
│             ToolApprovalCoordinator, ToolPermissionManager, ...
├── Checkpoint: CheckpointCoordinator, CheckpointState, ...
├── Metrics: MetricsRegistry, various collectors, ...
└── Utilities: ConversationSession, VariableManager, InterruptionState, ...
```

### Service Identifiers

All services are registered with string identifiers from `di/service-identifiers.ts`:

```typescript
export const Identifiers = {
  WorkflowGraphRegistry: "workflowGraphRegistry",
  WorkflowExecutionRegistry: "workflowExecutionRegistry",
  WorkflowLifecycleCoordinator: "workflowLifecycleCoordinator",
  WorkflowStateTransitor: "workflowStateTransitor",
  // ... more identifiers
};
```

### Factory Pattern

The container uses factory functions for services that require dynamic configuration:

```typescript
container.registerFactory(Identifiers.WorkflowExecutor, () => {
  return new WorkflowExecutor({
    workflowGraphRegistry: container.resolve(Identifiers.WorkflowGraphRegistry),
    workflowExecutionCoordinatorFactory: {
      create: (entity) => new WorkflowExecutionCoordinator(entity, ...),
    },
  });
});
```

## 2. Plugin System

### Plugin Architecture

```
Plugin System
├── PluginRegistry (register and discover plugins)
├── PluginLoader (load plugins from configuration)
├── ContributionManager (manage plugin contributions)
├── PluginGuard (validate plugin permissions)
├── EventBus (plugin-to-plugin communication)
└── PluginConfig (plugin configuration)
```

### Plugin Types

| Plugin Type | Capabilities |
|-------------|-------------|
| **Node Handler Plugin** | Register custom node types and handlers |
| **Trigger Plugin** | Register custom trigger types |
| **Hook Plugin** | Register custom hook types |
| **Tool Plugin** | Register custom tools |
| **LLM Provider Plugin** | Register custom LLM providers |
| **Event Plugin** | Subscribe to execution events |

### Contribution Points

Plugins can contribute to:

- **Node Handlers**: Custom node types with dedicated handlers (3rd priority in handler resolution)
- **Trigger Handlers**: Custom trigger action handlers
- **Hook Templates**: Reusable hook configurations
- **Tool Definitions**: Custom tool implementations
- **Event Subscriptions**: React to execution events
- **Metrics Collectors**: Custom metrics collection

### Plugin Lifecycle

```
Plugin Loading Flow:
1. PluginLoader reads plugin configuration
2. PluginGuard validates permissions
3. PluginRegistry registers the plugin
4. ContributionManager loads contributions
5. Plugin is ready for use

Plugin Unloading Flow:
1. ContributionManager removes contributions
2. PluginRegistry unregisters the plugin
3. Cleanup resources
```

## 3. ContainerManager

`ContainerManager` manages the DI container lifecycle:

```
ContainerManager
├── initialize(config) → Initialize container with config
├── getContainer() → Get the IContainer instance
├── createChildContainer() → Create scoped container
├── shutdown() → Clean up all services
└── isInitialized() → Check initialization status
```