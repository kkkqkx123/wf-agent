# Workflow Core Data Model

## 1. WorkflowGraph

`WorkflowGraph` is the central data structure representing a workflow. It follows a **separation of concerns** design:

- **`WorkflowGraphStructureImpl`** — Immutable graph topology (nodes, edges, adjacency lists)
- **`WorkflowGraphMetadata`** — Preprocessing data (topological order, subgraph relationships, validation results)
- **`WorkflowGraph`** — Facade combining both, implementing `WorkflowGraphStructure` interface

```
WorkflowGraph
├── structure: WorkflowGraphStructureImpl  (immutable topology)
│   ├── nodes: Map<ID, WorkflowNode>
│   ├── edges: Map<ID, WorkflowEdge>
│   ├── adjacencyList: Map<ID, Set<ID>>
│   ├── reverseAdjacencyList: Map<ID, Set<ID>>
│   ├── startNodeId: ID?
│   ├── endNodeIds: Set<ID>
│   └── _outgoingEdgeMap / _incomingEdgeMap (indexed for O(degree) lookup)
│
└── metadata: WorkflowGraphMetadata  (preprocessing result)
    ├── topologicalOrder: ID[]
    ├── subgraphRelationships: SubgraphRelationship[]
    ├── subgraphMergeLogs: SubgraphMergeLog[]
    ├── graphAnalysis: WorkflowGraphAnalysis
    ├── validationResult: PreprocessValidationResult
    └── processedAt: Timestamp
```

### Key Design Decisions

- **Immutable structure**: `WorkflowGraphStructureImpl` is built once during construction, then remains read-only during execution
- **Config separation**: Node configurations (`StaticNode`) and trigger configurations (`WorkflowTrigger`) are stored separately from the graph structure
- **Preprocessing data**: Analysis results (topological sort, reachability, cycle detection) are computed once and stored in metadata

## 2. WorkflowExecutionEntity

`WorkflowExecutionEntity` is the **stateful** runtime representation of a single workflow execution. It implements `IExecutionEntity` and manages:

```
WorkflowExecutionEntity
├── identity: id, executionId, workflowId, workflowVersion
├── status: WorkflowExecutionStatus (managed by WorkflowExecutionState)
├── graph: WorkflowGraph (read-only reference)
├── state: WorkflowExecutionState  (status transitions, error records, interruption history)
├── executionState: ExecutionState  (subgraph stack, current workflow context)
├── forkJoinState: ForkJoinState  (fork path tracking, aggregation state)
├── variableManager: VariableManager  (scoped variables)
├── hierarchyManager: ExecutionHierarchyManager  (parent-child relationships)
├── timeoutManager: TimeoutManager  (per-node and wall-clock timeout)
├── conversationSession: ConversationSession  (message history)
├── interruptionManager: InterruptionState  (pause/stop signals)
├── syncBarrier: SyncBarrier  (fork branch synchronization)
├── toolFailureProtectionState: ToolFailureProtectionState
├── eventRegistry: EventRegistry
├── retryBudget: RetryBudget
└── checkpointDependencies: CheckpointDependencies
```

### State Management

Three separate state managers, each with `StateManager<T>` interface (snapshot/restore for checkpoint):

| State Manager | Responsibility | Snapshot Type |
|--------------|---------------|---------------|
| `WorkflowExecutionState` | Status transitions, error records, interruption history | `WorkflowExecutionStateSnapshot` |
| `ExecutionState` | Subgraph execution stack | `SubgraphContext[]` |
| `ForkJoinState` | Fork path tracking, JOIN aggregation | `ForkJoinStateSnapshot` |

## 3. Node Type System

### StaticNode vs RuntimeNode

The system has a **two-phase node type system**:

```
StaticNode (TOML/config definition)
  │
  ├── Preprocessing (subgraph expansion, validation)
  │
  └── RuntimeNode (execution graph)
        │
        └── WorkflowNode (RuntimeNode + originalNode reference)
```

**StaticNode Types** (16 types):

| Category | Types |
|----------|-------|
| **Boundary** | `START`, `END`, `START_FROM_TRIGGER`, `CONTINUE_FROM_TRIGGER` |
| **Control Flow** | `ROUTE`, `FORK`, `JOIN`, `SYNC`, `LOOP_START`, `LOOP_END` |
| **Execution** | `SCRIPT`, `INTERACTIVE_SCRIPT`, `LLM`, `AGENT_LOOP` |
| **Data** | `VARIABLE`, `CONTEXT_PROCESSOR` |
| **Composition** | `SUBGRAPH`, `EMBED_GRAPH` |
| **Interaction** | `USER_INTERACTION`, `TOOL_VISIBILITY` |

**RuntimeNode Types** add 2 internal types:

- `EMBED_START` / `EMBED_END` — Generated during `EMBED_GRAPH` expansion (Phase 3 preprocessing)

### Preprocessing Phases

1. **Phase 1 — SUBGRAPH handling**: SUBGRAPH nodes remain in the runtime graph, executed as independent execution entities (Scheme C)
2. **Phase 2 — EMBED_GRAPH expansion**: EMBED_GRAPH nodes are expanded inline during preprocessing, their START/END become EMBED_START/EMBED_END
3. **Phase 3 — Validation and analysis**: Topological sort, cycle detection, reachability analysis