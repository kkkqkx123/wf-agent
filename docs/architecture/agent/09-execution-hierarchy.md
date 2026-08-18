# Agent Execution Hierarchy

## 1. Overview

The agent loop execution hierarchy manages parent-child relationships between agent loop executions and other execution types (workflows). This enables:

- **Nested agent loops**: Agent loops triggered within other agent loops
- **Agent-in-workflow**: Agent loops executed as sub-steps of workflows
- **Workflow-in-agent**: Workflows triggered from agent tool calls
- **Hierarchical checkpointing**: Child execution state restoration

## 2. Architecture

```
AgentLoopEntity
├── hierarchyManager: ExecutionHierarchyManager
│   ├── getParentContext() → ParentExecutionContext?
│   ├── getChildExecutions() → ChildExecutionReference[]
│   ├── addChildExecution(ref) → void
│   └── removeChildExecution(id) → void
│
└── nodeId: ID?  (node ID in parent workflow if applicable)
```

### ExecutionHierarchyManager (shared)

Manages the parent-child relationship metadata:

```
ExecutionHierarchyManager
├── parentContext: ParentExecutionContext?
│   ├── parentType: "WORKFLOW" | "AGENT_LOOP"
│   ├── parentId: ID
│   └── nodeId?: ID  (node in parent that created this execution)
│
├── children: Map<ID, ChildExecutionReference>
│   ├── childId: ID
│   ├── childType: "WORKFLOW" | "AGENT_LOOP"
│   ├── createdAt: timestamp
│   └── status: ExecutionStatus
│
├── addParentContext(parent) → void
├── getParentContext() → ParentExecutionContext?
├── addChildExecution(ref) → void
├── getChildExecutions() → ChildExecutionReference[]
└── removeChildExecution(id) → void
```

## 3. Triggered Agent Execution Manager

`TriggeredAgentExecutionManager` manages the execution of triggered (nested) agent loop executions. It mirrors the design of `TriggeredWorkflowExecutionManager` for symmetry.

### Design Principles

- **Single responsibility**: Manage triggered agent loop executions
- **Simplified pending queue**: FIFO, no complex state tracking
- **Direct integration with TaskRegistry**: Single source of truth for state
- **Delegates executor handling to coordinator callback**
- **Mirrors**: TriggeredWorkflowExecutionManager (symmetric design pattern)

### Architecture

```
TriggeredAgentExecutionManager (implements TaskManager)
├── submitTask(entity, config, options?) → TaskSubmissionResult
│   ├── Create task in TaskRegistry
│   ├── Add to pending queue (FIFO)
│   ├── If sync: wait for completion
│   └── If async: return immediately
│
├── executeNextPending() → void
│   ├── Dequeue next pending task
│   ├── Execute via executor callback
│   └── Resolve/reject task promise
│
├── cancelTask(taskId) → boolean
│   ├── Cancel pending task
│   └── Clean up resources
│
├── getTaskStatus(taskId) → TaskStatus
│   └── Query TaskRegistry
│
└── cleanup() → void
    └── Clear all pending tasks
```

### Triggered Execution Config

```typescript
interface TriggeredAgentExecutionConfig {
  executionId: string;
  parentEntity: any;
  parentType: "WORKFLOW" | "AGENT_LOOP";
  timeout?: number;
  waitForCompletion?: boolean;
}
```

### Execution Flow

```
Parent workflow/agent → Trigger matched
  → TriggeredAgentExecutionManager.submitTask()
    → Create task in TaskRegistry
    → Add to pending queue
    → Execute via executor callback
      → AgentLoopCoordinator.execute()
        → AgentLoopFactory.create()
        → AgentLoopExecutor.execute()
        → Register child in ExecutionHierarchyManager
        → Return result
    → Resolve task promise
    → Return result to parent
```

## 4. Hierarchy Registration

When an agent loop is created within a parent context:

```
AgentLoopFactory.create(config, options):
  ...
  if (options.parentExecutionId):
    // Register with ExecutionHierarchyRegistry
    hierarchyRegistry.registerChild(parentId, childId, "AGENT_LOOP")
    // Set parent context in entity
    entity.hierarchyManager.addParentContext({
      parentType: "WORKFLOW" | "AGENT_LOOP",
      parentId: options.parentExecutionId,
      nodeId: options.nodeId,
    })
  ...
```

## 5. Hierarchy Integrity Service

The `HierarchyIntegrityService` (shared) ensures consistency of the execution hierarchy:

```
HierarchyIntegrityService
├── validateHierarchy(entity) → boolean
│   ├── Check parent exists
│   ├── Check child references are valid
│   └── Check no circular dependencies
│
├── repairHierarchy(entity) → void
│   └── Remove orphaned references
│
└── cleanupOrphanedChildren(parentId) → void
    └── Remove children of a completed parent
```

## 6. Checkpoint Restoration with Hierarchy

When restoring from a checkpoint, child executions are restored:

```
AgentLoopCheckpointCoordinator.restoreFromCheckpoint():
  1. Restore parent entity
  2. Find child executions via ExecutionHierarchyRegistry
  3. For each child:
     a. Resolve latest checkpoint
     b. Restore child entity
     c. Rebuild parent-child relationships
  4. Verify hierarchy integrity
```

## 7. Relationship Types

| Parent Type | Child Type | Use Case |
|-------------|-----------|----------|
| WORKFLOW | AGENT_LOOP | Agent node in workflow |
| AGENT_LOOP | AGENT_LOOP | Nested agent loop (triggered) |
| AGENT_LOOP | WORKFLOW | Workflow triggered from agent tool |