# Agent DI Container and Plugin System

## 1. Dependency Injection

The agent module integrates with the shared DI container for service registration and resolution.

### Agent-Specific Service Registrations

The DI container (configured in `packages/sdk/di/container-config.ts`) registers the following agent services:

```
Agent Registrations:
├── Registries:
│   ├── AgentLoopRegistry (singleton)
│   └── IAgentExecutionRegistry (interface → AgentLoopRegistry)
│
├── Coordinators:
│   ├── AgentLoopCoordinator (lifecycle management)
│   ├── ConversationCoordinator (conversation management)
│   ├── AgentLoopStateTransitor (state transitions)
│   └── AgentLoopCheckpointCoordinator (checkpoint management)
│
├── Executors:
│   ├── AgentLoopExecutor (execution entry point)
│   └── AgentLoopFactory (entity creation)
│
├── Checkpoint:
│   ├── AgentLoopCheckpointCoordinator
│   ├── AgentLoopCheckpointStateManager
│   └── AgentLoopCheckpointConfigResolver
│
└── Metrics:
    ├── AgentLoopMetricsCollector (agent-specific metrics)
    └── AgentPerformanceAnalyzer (performance analysis)
```

### Service Identifiers

```typescript
// From packages/sdk/di/service-identifiers.ts
export const Identifiers = {
  // Agent services
  AgentLoopCoordinator: "agentLoopCoordinator",
  AgentLoopExecutor: "agentLoopExecutor",
  AgentLoopFactory: "agentLoopFactory",
  AgentLoopRegistry: "agentLoopRegistry",
  AgentLoopStateTransitor: "agentLoopStateTransitor",
  AgentLoopCheckpointCoordinator: "agentLoopCheckpointCoordinator",
  ConversationCoordinator: "conversationCoordinator",
  AgentLoopMetricsCollector: "agentLoopMetricsCollector",

  // Shared services used by agent
  LLMExecutionCoordinator: "llmExecutionCoordinator",
  ToolCallExecutor: "toolCallExecutor",
  LLMWrapper: "llmWrapper",
  ToolRegistry: "toolRegistry",
  EventRegistry: "eventRegistry",
  // ... more shared identifiers
};
```

### Factory Pattern

The container uses factory functions for services that require dynamic configuration:

```typescript
container.registerFactory(Identifiers.AgentLoopCoordinator, () => {
  return new AgentLoopCoordinator({
    agentLoopFactory: container.resolve(Identifiers.AgentLoopFactory),
    agentLoopExecutor: container.resolve(Identifiers.AgentLoopExecutor),
    agentLoopRegistry: container.resolve(Identifiers.AgentLoopRegistry),
    stateTransitor: container.resolve(Identifiers.AgentLoopStateTransitor),
    checkpointCoordinator: container.resolve(Identifiers.AgentLoopCheckpointCoordinator),
    eventManager: container.resolve(Identifiers.EventRegistry),
    // ... more dependencies
  });
});
```

### Container Configuration

```typescript
function configureContainerBindings(container: Container): void {
  // Agent registries
  container.registerSingleton(Identifiers.AgentLoopRegistry, AgentLoopRegistry);

  // Agent coordinators
  container.registerFactory(Identifiers.AgentLoopCoordinator, () => { ... });
  container.registerFactory(Identifiers.ConversationCoordinator, () => { ... });

  // Agent executors
  container.registerFactory(Identifiers.AgentLoopExecutor, () => { ... });
  container.registerFactory(Identifiers.AgentLoopFactory, () => { ... });

  // Agent checkpoint
  container.registerFactory(Identifiers.AgentLoopCheckpointCoordinator, () => { ... });
}
```

## 2. Plugin System

The plugin system (shared) allows external modules to extend agent functionality.

### Plugin Architecture

```
Plugin (interface)
├── id: string
├── name: string
├── version: string
├── onInitialize(context) → Promise<void>
├── onActivate(context) → Promise<void>
├── onDeactivate(context) → Promise<void>
└── onDestroy(context) → Promise<void>
```

### Agent Plugin Extension Points

Plugins can extend agent functionality at the following points:

| Extension Point | Description |
|----------------|-------------|
| **Service Registration** | Register additional services in the DI container |
| **Event Subscription** | Subscribe to agent events |
| **Hook Registration** | Register custom hooks |
| **Tool Registration** | Register custom tools |
| **Trigger Registration** | Register custom triggers |
| **Metrics Collection** | Add custom metrics collectors |

### Plugin Lifecycle

```
PluginManager (shared)
├── register(plugin) → void
├── activate(pluginId) → void
├── deactivate(pluginId) → void
└── unregister(pluginId) → void
```

### Agent-Specific Plugin Integration

```
AgentLoopCoordinator initialization:
  1. Resolve agent services from DI container
  2. Activate registered plugins
  3. Plugin hook registration:
     - BEFORE_ITERATION → custom logic
     - AFTER_ITERATION → custom logic
  4. Tool registration:
     - Builtin tools
     - Plugin-provided tools
  5. Return initialized coordinator
```

## 3. Service Dependencies

### AgentLoopCoordinator Dependencies

```
AgentLoopCoordinator
├── agentLoopFactory: AgentLoopFactory
├── agentLoopExecutor: AgentLoopExecutor
├── agentLoopRegistry: AgentLoopRegistry
├── stateTransitor: AgentLoopStateTransitor
├── checkpointCoordinator: AgentLoopCheckpointCoordinator
├── stateCoordinatorFactory: () => AgentStateCoordinator
├── interruptionStateFactory: InterruptionStateFactory
├── eventManager: EventRegistry
├── metricsCollector?: AgentLoopMetricsCollector
├── globalContext?: GlobalContext
└── checkpointPolicy?: AgentCheckpointPolicy
```

### AgentLoopExecutor Dependencies

```
AgentLoopExecutor
├── llmExecutor: LLMExecutor
├── toolService: ToolRegistry
├── eventManager?: EventRegistry
├── toolApprovalHandler?: ToolApprovalHandler
├── metricsRegistry?: MetricsRegistry
├── globalContext?: GlobalContext
└── checkpointDependencies?: WorkflowCheckpointDependencies
```

## 4. Inter-Service Communication

Services communicate via:

1. **Direct DI injection**: Coordinators receive dependencies via constructor
2. **Event system**: Decoupled communication via EventRegistry
3. **Registry access**: Shared state via registries (AgentLoopRegistry, ToolRegistry, etc.)
4. **Command pattern**: API layer dispatches to coordinators via commands