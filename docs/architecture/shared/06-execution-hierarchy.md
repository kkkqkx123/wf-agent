# Shared Execution Hierarchy

## 1. Overview

The execution hierarchy system manages parent-child relationships between execution entities (workflows, agent loops). It enables hierarchical execution tracking, checkpoint restoration, and lifecycle management.

## 2. ExecutionHierarchyManager

Manages parent-child relationship metadata within an execution entity:

```
ExecutionHierarchyManager
├── Parent Context
│   ├── parentContext: ParentExecutionContext?
│   ├── setParentContext(parent) → void
│   ├── getParentContext() → ParentExecutionContext?
│   ├── hasParent() → boolean
│   └── clearParentContext() → void
│
├── Child Management
│   ├── children: Map<ID, ChildExecutionReference>
│   ├── addChildExecution(ref) → void
│   ├── getChildExecutions() → ChildExecutionReference[]
│   ├── getChildExecution(id) → ChildExecutionReference?
│   ├── removeChildExecution(id) → boolean
│   ├── hasChildren() → boolean
│   └── getChildCount() → number
│
├── Metadata
│   ├── getHierarchyMetadata() → ExecutionHierarchyMetadata
│   └── getDepth() → number
│
└── Checkpoint Support
    ├── createSnapshot() → HierarchySnapshot
    └── restoreFromSnapshot(snapshot) → void
```

### ParentExecutionContext

```typescript
interface ParentExecutionContext {
  parentType: "WORKFLOW" | "AGENT_LOOP";
  parentId: ID;
  nodeId?: ID;  // Node in parent that created this execution
  metadata?: Record<string, unknown>;
}
```

### ChildExecutionReference

```typescript
interface ChildExecutionReference {
  childId: ID;
  childType: "WORKFLOW" | "AGENT_LOOP";
  createdAt: number;
  status: ExecutionStatus;
  metadata?: Record<string, unknown>;
}
```

## 3. ExecutionHierarchyRegistry

The global registry for tracking all parent-child relationships:

```
ExecutionHierarchyRegistry
├── Registration
│   ├── registerChild(parentId, childId, childType) → void
│   ├── unregisterChild(parentId, childId) → void
│   ├── registerParent(childId, parentId) → void
│   └── unregisterParent(childId) → void
│
├── Query
│   ├── getChildren(parentId) → ChildExecutionReference[]
│   ├── getParent(childId) → ParentExecutionReference | null
│   ├── getSiblings(executionId) → ChildExecutionReference[]
│   ├── hasChildren(parentId) → boolean
│   ├── hasParent(childId) → boolean
│   └── getAncestors(executionId) → ExecutionReference[]
│
├── Traversal
│   ├── getDescendantCount(executionId) → number
│   ├── getSubtree(executionId) → ExecutionTreeNode
│   └── getPathToRoot(executionId) → ExecutionReference[]
│
└── Maintenance
    ├── cleanupOrphanedChildren(parentId) → void
    ├── getAllRootNodes() → ExecutionReference[]
    └── clear() → void
```

## 4. HierarchyIntegrityService

Ensures consistency of the execution hierarchy:

```
HierarchyIntegrityService
├── validateHierarchy(entity) → ValidationResult
│   ├── Check parent exists (if parent declared)
│   ├── Check child references are valid
│   ├── Check no circular dependencies
│   └── Return validation result with errors
│
├── repairHierarchy(entity) → RepairResult
│   ├── Remove orphaned references
│   ├── Fix broken parent-child links
│   └── Return repair summary
│
├── cleanupOrphanedChildren(parentId) → number
│   └── Remove children of a completed/cancelled parent
│
├── validateNoCycles(executionId) → boolean
│   └── DFS cycle detection
│
└── getIntegrityReport() → IntegrityReport
    └── Full hierarchy integrity report
```

## 5. Hierarchy Traversal

### HierarchyTraversalService

```
HierarchyTraversalService
├── traverseUp(executionId, callback) → void
│   └── Walk up the parent chain, calling callback at each level
│
├── traverseDown(executionId, callback) → void
│   └── Walk down the child tree, calling callback at each level
│
├── findAncestor(executionId, predicate) → ExecutionReference | null
│   └── Find first ancestor matching predicate
│
├── findDescendant(executionId, predicate) → ExecutionReference | null
│   └── Find first descendant matching predicate
│
├── getPathToRoot(executionId) → ExecutionReference[]
│   └── Get ordered list from execution to root
│
└── getDepth(executionId) → number
    └── Get depth from root
```

## 6. Execution Hierarchy in Checkpoint

During checkpoint restoration, the hierarchy is reconstructed:

```
1. Restore parent entity
2. Query ExecutionHierarchyRegistry for children
3. For each child:
   a. Find latest checkpoint
   b. Restore child entity
   c. Rebuild parent-child relationships
4. Verify hierarchy integrity
```

## 7. Relationship Patterns

| Pattern | Description |
|---------|-------------|
| **Workflow → Agent** | Agent node in workflow creates an agent loop |
| **Agent → Agent** | Agent loop triggers a nested agent loop |
| **Agent → Workflow** | Agent loop tool triggers a workflow |
| **Workflow → Workflow** | Subgraph/sub-workflow nodes |