# Agent Builder and Configuration

## 1. Overview

The agent module provides multiple layers for creating and configuring agent loop instances:

- **API Layer**: Fluent builders for definition and runtime configuration
- **Factory Layer**: Entity creation, checkpoint restoration, clone operations
- **Config Resolution**: Checkpoint config resolution with layered configuration

## 2. API Builders

### AgentDefinitionBuilder

A fluent builder for creating static `AgentLoopDefinition` objects (used for config-based definitions):

```
AgentDefinitionBuilder
├── id(id) → this
├── name(name) → this
├── version(version) → this
├── description(desc) → this
├── profileId(id) → this
├── systemPrompt(prompt) → this
├── systemPromptTemplateId(id) → this
├── systemPromptTemplateVariables(vars) → this
├── maxIterations(n) → this
├── initialMessages(msgs) → this
├── availableTools(config) → this
├── stream(enabled) → this
├── hooks(hooks) → this
├── triggers(triggers) → this
├── dynamicContext(config) → this
├── checkpoint(config) → this
├── metadata(data) → this
└── build() → AgentLoopDefinition
```

### AgentLoopConfigBuilder

A fluent builder for creating runtime `AgentLoopRuntimeConfig` objects:

```
AgentLoopConfigBuilder
├── agentConfigId(id) → this
├── profileId(id) → this
├── systemPrompt(prompt) → this
├── systemPromptTemplateId(id) → this
├── systemPromptTemplateVariables(vars) → this
├── initialUserMessage(msg) → this
├── maxIterations(n) → this
├── initialMessages(msgs) → this
├── availableTools(config) → this
├── stream(enabled) → this
├── createCheckpointOnEnd(enabled) → this
├── createCheckpointOnError(enabled) → this
├── hooks(hooks) → this
├── triggers(triggers) → this
├── metadata(data) → this
└── build() → AgentLoopRuntimeConfig
```

### Specialized Builders

Additional builders for specific configuration aspects:

| Builder | Purpose |
|---------|---------|
| `AgentHookBuilder` | Build hook configurations with condition and action |
| `AgentToolConfigBuilder` | Build tool access configuration (include/exclude) |
| `AgentTriggerBuilder` | Build trigger configurations |

## 3. AgentLoopFactory

`AgentLoopFactory` is the central factory for creating `AgentLoopEntity` instances:

```
AgentLoopFactory
├── create(config, options?) → AgentLoopEntity
│   ├── Resolve system prompt
│   ├── Create ConversationSession
│   ├── Create AgentStateCoordinator
│   ├── Create AgentLoopEntity
│   ├── Register with AgentLoopRegistry
│   └── Register with ExecutionHierarchyRegistry (if parent exists)
│
├── fromCheckpoint(config, checkpoint, options?) → AgentLoopEntity
│   ├── Resolve checkpoint data
│   ├── Restore AgentLoopState from snapshot
│   ├── Create runtime managers
│   └── Build entity with restored state
│
├── fromConversationHistory(config, history, options?) → AgentLoopEntity
│   ├── Restore from conversation history (without full checkpoint)
│   └── Partial state recovery
│
├── clone(entity, options?) → AgentLoopEntity
│   └── Deep copy with optional overrides
│
└── buildEntity(config, state, options?) → AgentLoopEntity
    └── Core entity building logic
```

### Entity Options

```typescript
interface AgentLoopEntityOptions {
  initialMessages?: LLMMessage[];
  conversationManager?: ConversationSession;
  parentExecutionId?: ID;
  nodeId?: ID;
}
```

### Factory Design Principles

- **Centralized creation**: All entity creation goes through the factory
- **Decoupled from entity**: Factory manages wiring, entity is pure data
- **Parent-child management**: Automatic registration with hierarchy registry
- **Multiple creation paths**: New, from checkpoint, from history, clone

## 4. Checkpoint Config Resolution

The checkpoint configuration uses a layered resolution approach:

```
AgentLoopCheckpointConfigContext
├── Global config (from DI/container)
├── Agent-specific config (from AgentLoopRuntimeConfig)
├── Per-execution config (from options)
└── Resolved: AgentLoopCheckpointConfig
```

The `resolveAgentCheckpointConfig()` function merges these layers with precedence:

1. Per-execution options (highest priority)
2. Agent-specific config
3. Global defaults (lowest priority)

## 5. Lifecycle Functions

The `agent-loop-lifecycle.ts` module provides standalone lifecycle management functions:

| Function | Purpose |
|----------|---------|
| `createAgentLoopCheckpoint()` | Create checkpoint with coordinated dependencies |
| `cleanupAgentLoop()` | Clean up resources and registry entries |
| `cloneAgentLoop()` | Clone an entity with optional state override |