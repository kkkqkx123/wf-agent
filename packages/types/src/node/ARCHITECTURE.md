# Node Type Architecture - Static vs Runtime Separation

## Overview

This document describes the new node type architecture that clearly separates static (configuration-time) and runtime (execution-time) node types.

## Design Principles

### 1. Clear Phase Separation

- **Static Phase**: Workflow definition, validation, preprocessing
- **Runtime Phase**: Execution after preprocessing

These two phases **never mix** - each uses its own dedicated type system.

### 2. SUBGRAPH Node Lifecycle

```
Static Definition (TOML):          Runtime Execution (After Preprocessing):
[START] -> [SUBGRAPH: child]       [START] -> [SUBGRAPH_START] -> [LLM] 
         -> [END]                            -> [SUBGRAPH_END] -> [END]
```

- `SUBGRAPH` exists **only** in static definitions
- After preprocessing, it's expanded into its internal nodes
- Subgraph's START/END become `SUBGRAPH_START`/`SUBGRAPH_END`

### 3. Property Classification

#### Static-Only Properties (Display/CRUD)
- `name` - Human-readable name for UI
- `description` - Documentation/tooltips
- `metadata` - User-defined tags, filtering

**NOT used at runtime** - execution engine doesn't need these.

#### Execution Configuration (Defined statically, used at runtime)
- `hooks` - Lifecycle event handlers
- `checkpointBeforeExecute` - Checkpoint control
- `checkpointAfterExecute` - Checkpoint control

These are **defined in TOML** but **read by execution engine**.

#### Runtime-Only Context (Injected during preprocessing)
- `internalMetadata` - System-generated tracing data
- `originalNode` - Reference back to static definition
- `workflowId` / `parentWorkflowId` - Execution context
- `outgoingEdgeIds` / `incomingEdgeIds` - Graph structure

These **do NOT exist** in static definitions.

## File Structure

```
packages/types/src/node/
├── shared-node-types.ts      # Shared base interfaces
│   ├── NodeIdentity          # id + type
│   ├── StaticNodeDisplayProps # name, description, metadata
│   ├── NodeExecutionConfig   # hooks, checkpoints
│   └── RuntimeNodeContext    # internalMetadata, workflowId, edges
│
├── static-node-types.ts      # Static phase types
│   ├── StaticNodeType        # All types including SUBGRAPH
│   ├── BaseStaticNode        # Identity + Display + Execution config
│   ├── StaticNode            # Union of all static nodes
│   └── Type guards
│
├── runtime-node-types.ts     # Runtime phase types
│   ├── RuntimeNodeType       # All types EXCEPT SUBGRAPH, plus SUBGRAPH_START/END
│   ├── BaseRuntimeNode       # Identity + Execution config + Runtime context
│   ├── RuntimeNode           # Union of all runtime nodes
│   └── Type guards
│
├── runtime/                  # Runtime-only configurations (deprecated, removed in v2.0)
│   └── (previously contained subgraph-runtime-configs.ts)
│
└── base.ts                   # Backward compatibility exports
```

## Usage Examples

### Static Phase (Validation/Preprocessing)

```typescript
import type { StaticNode, SubgraphNode } from '@wf-agent/types';

// Validate workflow configuration
function validateWorkflow(nodes: StaticNode[]): void {
  for (const node of nodes) {
    if (isSubgraphNode(node)) {
      // SUBGRAPH exists only in static phase
      validateSubgraphConfig(node.config);
    }
  }
}

// Preprocess: expand SUBGRAPH nodes
function preprocessGraph(staticNodes: StaticNode[]): RuntimeNode[] {
  const runtimeNodes: RuntimeNode[] = [];
  
  for (const node of staticNodes) {
    if (isSubgraphNode(node)) {
      // Expand subgraph - replace with internal nodes
      const expanded = expandSubgraph(node);
      runtimeNodes.push(...expanded);
    } else {
      // Convert other nodes to runtime format
      runtimeNodes.push(convertToRuntimeNode(node));
    }
  }
  
  return runtimeNodes;
}
```

### Runtime Phase (Execution)

```typescript
import type { RuntimeNode, SubgraphStartNode } from '@wf-agent/types';

// Execute a node
async function executeNode(node: RuntimeNode): Promise<void> {
  // Read execution config
  if (node.checkpointBeforeExecute) {
    await createCheckpoint();
  }
  
  // Access runtime context
  console.log(`Executing in workflow: ${node.workflowId}`);
  
  // Handle subgraph boundaries
  if (isSubgraphStartNode(node)) {
    await enterSubgraphScope(node);
  }
  
  // Execute handler...
}
```

## Key Differences: Static vs Runtime

| Aspect | StaticNode | RuntimeNode |
|--------|-----------|-------------|
| **Phase** | Configuration/Validation | Execution |
| **SUBGRAPH type** | ✅ Exists | ❌ Expanded |
| **SUBGRAPH_START/END** | ❌ Doesn't exist | ✅ Generated |
| **name/description** | ✅ Present | ❌ Removed |
| **metadata** | ✅ User-defined | ❌ Not needed |
| **internalMetadata** | ❌ Doesn't exist | ✅ System-generated |
| **workflowId** | ❌ Not yet assigned | ✅ Assigned |
| **edge IDs** | ❌ Not computed | ✅ Computed |
| **originalNode** | N/A | ✅ Reference to static |

## Migration Guide

### For Existing Code

Most existing code uses the legacy `Node` type, which is now aliased to `StaticNode`:

```typescript
// Old code (still works)
import type { Node } from '@wf-agent/types';

// New code (recommended)
import type { StaticNode } from '@wf-agent/types';  // For validation/preprocessing
import type { RuntimeNode } from '@wf-agent/types'; // For execution
```

### Updating Handlers

```typescript
// Old handler signature
async function handleNode(node: Node): Promise<void> { ... }

// New handler signature (runtime)
async function handleNode(node: RuntimeNode): Promise<void> { ... }
```

## Benefits

1. **Type Safety**: Compile-time errors if you try to use static-only properties at runtime
2. **Clarity**: Clear separation of concerns between phases
3. **Performance**: Runtime nodes don't carry unnecessary display metadata
4. **Maintainability**: Easier to understand which properties are available when
5. **Correctness**: SUBGRAPH lifecycle is explicitly modeled in the type system

## Future Work

- Add Zod schemas for runtime node validation
- Create migration tools to update existing code
- Add runtime type checking utilities
- Document all node handlers with correct type signatures
