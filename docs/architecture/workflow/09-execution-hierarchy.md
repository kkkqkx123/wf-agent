# Execution Hierarchy

## 1. Overview

The execution hierarchy system manages parent-child relationships between execution instances. It supports mixed hierarchies where a workflow can contain agent loops, and an agent loop can trigger sub-workflows.

```
Workflow A (root)
├── SUBGRAPH → Workflow A.1 (child)
│   └── AGENT_LOOP → Agent Loop A.1.1 (grandchild)
├── FORK → Workflow A.2 (fork branch)
│   └── TRIGGERED → Workflow A.2.1 (triggered sub-workflow)
└── AGENT_LOOP → Agent Loop A.1
    └── TRIGGERED → Workflow A.1.1 (triggered from agent)
```

## 2. ExecutionHierarchyManager

Each execution entity (Workflow or Agent) has its own `ExecutionHierarchyManager` instance:

```typescript
ExecutionHierarchyManager
├── executionId: ID
├── executionType: ExecutionType (WORKFLOW | AGENT_LOOP)
├── parent: ParentExecutionContext?  (parent's execution info)
├── children: Map<string, ChildExecutionReference>  (child references)
├── depth: number  (cached, 0 for root)
├── rootExecutionId: ID  (cached root)
└── rootExecutionType: ExecutionType  (cached root type)
```

### Key Operations

- `setParent(parentContext)` → Set parent (with cycle detection)
- `addChild(childRef)` → Register child execution
- `removeChild(childId, childType)` → Unregister child
- `getDepth()` → Get cached depth (O(1))
- `getRootExecutionId()` → Get cached root execution ID

## 3. ExecutionHierarchyRegistry

A global registry for managing all execution instances:

```
ExecutionHierarchyRegistry
├── register(entity) → Register execution instance
├── unregister(executionId) → Remove from registry
├── get(executionId) → Get execution instance
├── getDescendants(executionId) → Recursive descendant query
├── cleanup(executionId) → Cascade cleanup of entire hierarchy
└── getExecutionsByRoot(executionId) → Group by root
```

## 4. Hierarchy Traversal

Delegated to `HierarchyTraversalService`:

- Deep-first traversal of the execution tree
- Bulk operations by root execution
- Mixed-type traversal (Workflow + Agent)

## 5. Hierarchy Integrity

Delegated to `HierarchyIntegrityService`:

- `validateHierarchy()` → Check consistency of parent-child relationships
- `repairHierarchy()` → Fix orphaned or inconsistent references
- `detectCycles()` → Prevent circular references

## 6. Child Execution Types

| Type | Description | Created By |
|------|-------------|------------|
| `SUBGRAPH` | Sub-workflow execution | `subgraphHandler` |
| `FORK_BRANCH` | Fork branch execution | `forkHandler` |
| `TRIGGERED` | Triggered sub-workflow execution | `TriggerCoordinator` |
| `AGENT_LOOP` | Agent loop execution | `agentLoopHandler` |

## 7. Child Cleanup

The `child-execution-cleanup.ts` utility provides:

- `cleanupChildExecution(executionId)` → Clean up a single child
- `cleanupSubworkflow(executionId)` → Sub-workflow-specific cleanup
- Resource cleanup: Close sessions, abort signals, clear registries