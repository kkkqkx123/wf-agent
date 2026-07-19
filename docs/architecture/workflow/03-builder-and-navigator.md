# Workflow Builder and Navigator

## 1. WorkflowGraphBuilder

`WorkflowGraphBuilder` is responsible for constructing `WorkflowGraph` objects from `WorkflowTemplate` definitions. It is a static utility class with three main operations:

### Build Pipeline

```
WorkflowTemplate
  │
  ├── WorkflowGraphBuilder.build()
  │   ├── Create WorkflowGraphStructureImpl
  │   ├── Add all nodes and edges
  │   ├── Process FORK/JOIN path IDs
  │   │   └── Assign unique forkPathId to each FORK branch
  │   └── Return WorkflowGraphStructureImpl
  │
  ├── WorkflowGraphBuilder.buildAndValidate()
  │   ├── Build graph structure
  │   ├── Run GraphValidator.validate()
  │   └── Return { graph, validationResult }
  │
  └── WorkflowGraphBuilder.processSubgraphs()
      ├── For each SUBGRAPH node:
      │   ├── Load sub-workflow template
      │   ├── Recursively process sub-workflows
      │   ├── Merge sub-graph into parent graph
      │   └── Record subgraph relationship
      ├── For each EMBED_GRAPH node:
      │   ├── Load embedded workflow template
      │   ├── Expand inline (START→EMBED_START, END→EMBED_END)
      │   └── Merge with renamed IDs
      └── Return merged WorkflowGraph
```

### Key Design Decisions

- **Static methods**: No instance state; all methods are pure transformations
- **Fork path IDs**: Each branch from a FORK node gets a unique `forkPathId` to track execution context
- **Subgraph preprocessing**: SUBGRAPH nodes are converted to independent execution entities at runtime (Scheme C)
- **Embed graph expansion**: EMBED_GRAPH nodes are expanded inline during preprocessing (Phase 3)

## 2. WorkflowNavigator

`WorkflowNavigator` provides graph traversal and routing logic during execution. It operates on an immutable `WorkflowGraphStructure` reference.

```
WorkflowNavigator
├── getNextNode(currentNodeId): NavigationResult
│   ├── Get outgoing edges of current node
│   ├── If single edge → return next node
│   ├── If multiple edges (ROUTE/FORK) → caller handles routing
│   └── If no edges → return END
│
├── routeNextNode(sourceNodeId, conditionResults): ID
│   ├── Evaluate condition-based routing
│   └── Return the matched target node ID
│
├── getPathTo(fromNodeId, targetNodeId): ID[] | null
│   └── BFS-based path finding for validation
│
├── getAllExecutionPaths(fromNodeId, options): PathEnumerationResult
│   └── DFS-based path enumeration for analysis
│
└── Graph query methods:
    ├── getPredecessors(nodeId): ID[]
    ├── getSuccessors(nodeId): ID[]
    ├── isForkNode(nodeId): boolean
    ├── isJoinNode(nodeId): boolean
    ├── isRouteNode(nodeId): boolean
    └── isEndNode(nodeId): boolean
```

## 3. Builder Utilities

### Workflow-Traversal Utilities

| Utility | Purpose |
|---------|---------|
| `workflow-cycle-detector.ts` | Detect cycles in the workflow graph |
| `workflow-graph-analyzer.ts` | Analyze graph structure (connected components, etc.) |
| `workflow-reachability-analyzer.ts` | Check node reachability from start |
| `workflow-topological-sorter.ts` | Topological sort of DAG |
| `workflow-traversal.ts` | BFS/DFS traversal utilities |