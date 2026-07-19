# Agent Registry and Storage

## 1. Registry Architecture

Registries provide runtime storage and lookup for agent loop execution artifacts.

### AgentLoopRegistry

The primary registry managing active `AgentLoopEntity` instances:

```
AgentLoopRegistry (implements IAgentExecutionRegistry)
├── Entity Storage (ExecutionStore<AgentLoopEntity>)
│   ├── register(entity) → void
│   ├── get(id) → AgentLoopEntity?
│   ├── unregister(id) → boolean
│   ├── has(id) → boolean
│   ├── getAll() → AgentLoopEntity[]
│   ├── query(filter?) → AgentLoopEntity[]
│   └── clear() → void
│
├── Coordinator Storage (CoordinatorStore<AgentStateCoordinator>)
│   ├── register(agentLoopId, coordinator) → void
│   ├── get(agentLoopId) → AgentStateCoordinator?
│   ├── unregister(agentLoopId) → boolean
│   └── clear() → void
│
├── State Coordinator Access
│   ├── getStateCoordinator(agentLoopId) → AgentStateCoordinator?
│   └── getConversationManager(agentLoopId) → ConversationSession?
│
├── Cleanup
│   ├── cleanupExpired(maxAge) → number (cleaned count)
│   └── cleanupCompleted(maxAge) → number (cleaned count)
│
└── Query
    ├── query(filter) → AgentLoopEntity[]
    ├── getByStatus(status) → AgentLoopEntity[]
    └── getByParentWorkflowId(workflowId) → AgentLoopEntity[]
```

### IAgentExecutionRegistry

Interface defining the contract for agent execution data access:

```typescript
interface IAgentExecutionRegistry {
  register(entity: AgentLoopEntity): void;
  get(id: ID): Promise<AgentLoopEntity | undefined>;
  unregister(id: ID): boolean;
  has(id: ID): boolean;
  query(filter?: AgentExecutionFilter): AgentLoopEntity[];
  getStateCoordinator(agentLoopId: ID): AgentStateCoordinator | null;
}
```

### AgentExecutionFilter

```typescript
interface AgentExecutionFilter {
  status?: string;               // Filter by status
  parentWorkflowId?: string;     // Filter by parent workflow
}
```

### ExecutionStore (shared base)

The `ExecutionStore` provides generic entity storage:

```
ExecutionStore<T>
├── register(entity) → void
├── get(id) → T?
├── unregister(id) → boolean
├── has(id) → boolean
├── getAll() → T[]
├── query(predicate?) → T[]
└── clear() → void
```

### CoordinatorStore (shared base)

The `CoordinatorStore` provides state coordinator storage:

```
CoordinatorStore<T>
├── register(entityId, coordinator) → void
├── get(entityId) → T?
├── unregister(entityId) → boolean
└── clear() → void
```

## 2. Agent Task Registry Integration

The `AgentLoopRegistry` also implements `AgentTaskManager` for async task management:

```
AgentTaskManager (interface)
├── cancelTask(taskId) → Promise<boolean>
└── getTaskStatus(taskId) → AgentTaskInfo | null

AgentTaskInfo
├── id: string
├── agentLoopId: ID
├── status: TaskStatus
├── submitTime: number
├── startTime?: number
├── completeTime?: number
├── result?: unknown
├── error?: Error
└── timeout?: number

AgentTaskStats
├── total: number
├── queued: number
├── running: number
├── completed: number
├── failed: number
├── cancelled: number
└── timeout: number
```

## 3. Storage Adaptors

The agent module integrates with storage adaptors for persistence:

```
Storage Adapters (from @wf-agent/storage):
├── AgentLoopStorageAdapter
│   ├── save(agentLoopEntity) → Promise<void>
│   ├── load(id) → Promise<AgentLoopEntity | null>
│   └── delete(id) → Promise<void>
│
├── CheckpointStorageAdapter
│   ├── save(checkpoint) → Promise<string>
│   ├── load(id) → Promise<Checkpoint | null>
│   ├── list(entityId) → Promise<string[]>
│   └── delete(id) → Promise<void>
│
└── AgentProfileStorageAdapter
    ├── save(profile) → Promise<void>
    ├── load(id) → Promise<AgentProfile | null>
    └── delete(id) → Promise<void>
```

## 4. Registry Design Principles

- **Composition**: AgentLoopRegistry composes ExecutionStore for entities and CoordinatorStore for coordinators
- **Singleton model**: Managed via DI container (single instance per application)
- **Thread-safe**: Map operations are safe for concurrent access
- **Expired instance cleanup**: `cleanupExpired()` and `cleanupCompleted()` for resource management
- **Workflow-execution-safe**: Consistent with WorkflowExecutionRegistry patterns

## 5. Agent Loop Metrics

The `AgentLoopMetricsCollector` tracks agent-specific metrics:

```
AgentLoopMetricsCollector
├── recordIteration(latency, tokenUsage) → void
├── recordToolCall(toolName, latency, success) → void
├── recordLLMCall(latency, tokenUsage) → void
├── recordError(errorType) → void
├── createSnapshot() → AgentLoopMetricsSnapshot
└── getMetricsReport() → AgentLoopMetricsReport
```

### Metrics Collected

| Metric | Description |
|--------|-------------|
| Iteration count | Total iterations per loop |
| Iteration latency | Time per iteration |
| LLM call latency | Time per LLM call |
| Tool call latency | Time per tool call |
| Token usage | Total tokens consumed |
| Error rate | Errors per iteration |
| Tool success rate | Tool call success rate |