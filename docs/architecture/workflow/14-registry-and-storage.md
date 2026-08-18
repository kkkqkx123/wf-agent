# Registry and Storage

## 1. Registry Architecture

Registries provide runtime storage and lookup for workflow execution artifacts.

### WorkflowRegistry

Stores workflow template definitions:

```
WorkflowRegistry
├── register(workflow) → Register workflow template
├── unregister(workflowId) → Remove workflow template
├── get(workflowId) → Get workflow template
├── getAll() → List all registered workflows
├── has(workflowId) → Check existence
└── clear() → Clear all registrations
```

### WorkflowGraphRegistry

Stores preprocessed workflow graphs (immutable after construction):

```
WorkflowGraphRegistry
├── register(workflowId, graph) → Register preprocessed graph
├── get(workflowId) → Get workflow graph
├── has(workflowId) → Check existence
├── unregister(workflowId) → Remove graph
└── clear() → Clear all graphs
```

### WorkflowExecutionRegistry

Manages active execution entities:

```
WorkflowExecutionRegistry
├── register(executionEntity) → Register execution
├── get(executionId) → Get execution entity
├── unregister(executionId) → Remove execution
├── getAll() → List all executions
├── getByWorkflowId(workflowId) → Query by workflow
├── getByStatus(status) → Query by status
└── clear() → Clear all executions
```

### WorkflowRelationshipRegistry

Tracks relationships between workflow definitions (for subgraph/sub-workflow references):

```
WorkflowRelationshipRegistry
├── registerRelationship(parentId, childId) → Register parent-child relationship
├── getChildren(parentId) → Get child workflow IDs
├── getParents(childId) → Get parent workflow IDs
├── hasRelationships(workflowId) → Check existence
└── removeRelationships(workflowId) → Remove all relationships
```

## 2. Task Registry

`TaskRegistry` manages triggered sub-workflow tasks:

```
TaskRegistry
├── submit(task) → Submit a new task
├── get(taskId) → Get task details
├── update(taskId, updates) → Update task state
├── complete(taskId, result) → Mark task as completed
├── fail(taskId, error) → Mark task as failed
├── cancel(taskId) → Cancel a task
├── list(filter) → Query tasks by filter
└── TaskManager interface → Pluggable task execution backends
```

## 3. Event Registry

`EventRegistry` is the central event bus:

```
EventRegistry
├── emit(event) → Publish event
├── subscribe(eventType, handler) → Subscribe
├── unsubscribe(eventType, handler) → Unsubscribe
├── getEventHistory(filter) → Query past events
├── clear() → Clear all events
└── getStats() → Event statistics
```

## 4. Storage Adapters

### Storage Interface

The system supports pluggable storage backends via `StorageAdapter`:

```typescript
interface StorageAdapter {
  // Checkpoint storage
  saveCheckpoint(workflowId, executionId, checkpoint): Promise<void>;
  loadCheckpoint(executionId): Promise<Checkpoint | null>;
  listCheckpoints(workflowId): Promise<Checkpoint[]>;
  deleteCheckpoint(executionId): Promise<void>;

  // Workflow template storage
  saveWorkflow(workflow): Promise<void>;
  loadWorkflow(workflowId): Promise<WorkflowTemplate | null>;
  listWorkflows(): Promise<WorkflowTemplate[]>;
  deleteWorkflow(workflowId): Promise<void>;
}
```

### Storage Adapter Types

| Adapter | Description |
|---------|-------------|
| **InMemoryStorageAdapter** | In-memory storage (default, for testing) |
| **FileStorageAdapter** | File-based storage (JSON files) |
| **Custom StorageAdapter** | User-provided storage backends |

### FileCheckpointManager

Manages file-based checkpoint persistence:

- Checkpoints stored as JSON files in configurable directory
- Supports checkpoint cleanup and budget management
- File naming convention: `{executionId}-{timestamp}.checkpoint.json`

## 5. Workflow Persistence

Workflow templates can be persisted to storage:

- `persistWorkflow(workflow)` → Save workflow to storage
- `loadWorkflow(workflowId)` → Load from storage
- `removeWorkflow(workflowId)` → Delete from storage
- `initializeWorkflowsFromStorage()` → Load all workflows on startup

## 6. Metrics Registry

`MetricsRegistry` collects execution metrics:

```
MetricsRegistry
├── NodeCollector (per-node execution metrics)
├── WorkflowCollector (per-workflow execution metrics)
├── AgentCollector (agent loop metrics)
├── ToolCollector (tool execution metrics)
├── TokenCollector (LLM token usage)
├── TimeoutCollector (timeout events)
├── RetryBudgetCollector (retry budget usage)
├── ResourceCollector (resource utilization)
├── ErrorCollector (error statistics)
├── EventCollector (event statistics)
└── ConfigCollector (configuration metrics)
```