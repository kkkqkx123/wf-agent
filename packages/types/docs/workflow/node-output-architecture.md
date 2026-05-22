# Node Output Architecture

## Overview

This document describes the node output architecture in the Modular Agent Framework.
Every control flow and execution node produces structured output data during workflow execution.
The output architecture defines how these outputs are typed, stored, sanitized, and referenced.

## Design Goals

1. **Type Safety**: Each node type has a well-defined output shape
2. **Path-Based Access**: All outputs are `Record<string, unknown>` for expression path resolution
3. **Extensibility**: New node types can define their own output shapes
4. **Configuration-Driven**: Output filtering is defined in node config, not hardcoded
5. **Validation Ready**: Static analysis can detect output path conflicts

## Key Concepts

### NodeExecutionResult.output

Every node execution produces a `NodeExecutionResult` stored in the workflow history.
The `output` field uses `Record<string, unknown>` to enforce string-keyed data path IDs.

```typescript
interface NodeExecutionResult {
  nodeId: ID;
  nodeType: string;
  status: "PENDING" | "RUNNING" | "COMPLETED" | "FAILED" | "SKIPPED" | "CANCELLED";
  step: number;
  output?: Record<string, unknown>;  // String-keyed data path
  // ... other fields
}
```

This design ensures:
- All output fields are string-keyed for expression resolution
- Type information is preserved at field level
- No ambiguity in data path syntax (`node.<outputId>.field`)

### NodeOutputConfig

Each node can configure its output behavior via `NodeOutputConfig`:

```typescript
interface NodeOutputConfig {
  /** Semantic ID for expression references (e.g. node.<outputId>.field) */
  outputId?: string;

  /** Override which fields to include in sanitized output */
  includeFields?: string[];

  /** Whether to exclude internal metadata (default: true) */
  excludeInternal?: boolean;
}
```

The `outputId` is used in conditions and expressions to reference a specific node's output.
For example: `node.llm_response.content` or `node.routing_decision.selectedRoute`.

### Output Field Mapping

Each node type has a default set of fields included in sanitized output.
These are defined in `DEFAULT_OUTPUT_FIELDS` and used by `sanitizeNodeOutput`.

| Node Type | Default Output Fields |
|-----------|----------------------|
| START | `startedAt` |
| END | `output` |
| VARIABLE | `variableName`, `oldValue`, `newValue` |
| FORK | `launchedBranches` |
| JOIN | `completedBranches`, `failedBranches`, `skippedBranches`, `strategy`, `aggregatedOutput` |
| SYNC | `syncedFromPath`, `syncedVariables`, `completed` |
| SUBGRAPH | `executionResult`, `duration` |
| SCRIPT | `result` |
| LLM | `content`, `toolCalls` |
| ADD_TOOL | `addedTools`, `scope` |
| TOOL_VISIBILITY | `action`, `toolIds`, `scope` |
| USER_INTERACTION | `operationType`, `userInput`, `updatedVariables`, `addedMessages` |
| ROUTE | `selectedRoute`, `evaluatedConditions` |
| CONTEXT_PROCESSOR | `operationsApplied`, `sourceContext`, `targetContext` |
| LOOP_START | `loopId`, `iterationCount`, `maxIterations`, `hasMoreIterations` |
| LOOP_END | `loopId`, `breakTriggered`, `iterationCount`, `nextIteration` |
| AGENT_LOOP | `finalResponse`, `toolCallCount`, `iterationCount` |
| START_FROM_TRIGGER | `startedAt` |
| CONTINUE_FROM_TRIGGER | `output` |
| EMBED_START | `startedAt` |
| EMBED_END | `output` |

### Sanitization Flow

```
Node Handler (raw output)
    ↓
sanitizeNodeOutput(raw, node)
    ├── null/undefined → {}
    ├── scalar → { result: value }
    ├── FORK array → { launchedBranches: [...] }
    ├── SUBGRAPH → { executionResult: {...}, duration }
    └── generic → filter by includeFields/DEFAULT_OUTPUT_FIELDS, strip internal keys
    ↓
NodeExecutionResult.output (Record<string, unknown>)
```

The sanitizer:
- Strips internal metadata (`nodeId`, `nodeType`, `status`, `startTime`, `endTime`, `timestamp`, `executionTime`, `step`, `error`)
- Uses `node.output.includeFields` as explicit override
- Falls back to `DEFAULT_OUTPUT_FIELDS[node.type]`
- Includes all non-internal fields when no field list is defined
- Wraps scalar values as `{ result: value }` to maintain `Record` contract
- Handles FORK/SUBGRAPH special cases for structural transformation

## File Structure

```
packages/types/src/node/
├── node-outputs.ts              # Output type interfaces + DEFAULT_OUTPUT_FIELDS
├── shared-node-types.ts         # NodeOutputConfig in NodeExecutionConfig
├── runtime-node-types.ts        # RuntimeNodeOutputOfType helper
├── configs/
│   ├── execution-configs.ts     # Script/LLM/AddTool/ToolVisibility output docs
│   ├── control-configs.ts       # ROUTE output docs
│   ├── variable-configs.ts      # VARIABLE output docs
│   ├── interaction-configs.ts   # USER_INTERACTION output docs
│   ├── fork-join-configs.ts     # FORK/JOIN output docs
│   ├── sync-configs.ts          # SYNC output docs
│   ├── loop-configs.ts          # LOOP_START/LOOP_END output docs
│   ├── context-configs.ts       # CONTEXT_PROCESSOR output docs
│   ├── subgraph-configs.ts      # SUBGRAPH output docs
│   ├── agent-loop-configs.ts    # AGENT_LOOP output docs
│   └── boundary-config.ts       # START/END output (workflow level)
```

## Expression Data Path Model

All outputs use `Record<string, unknown>` to support path-based access.

### Path Syntax

```
node.<outputId>.<field>
node.<outputId>.<nested.field>
```

### Evaluation Context Integration

The `EvaluationContext` used by condition evaluators includes:

```typescript
interface EvaluationContext {
  variables: Record<string, unknown>;
  input: Record<string, unknown>;
  output: Record<string, unknown>;
  // Node outputs are accessed through the output namespace
  // mapped by outputId during graph traversal
}
```

## Validation Requirements (Not Yet Implemented)

### Static Validation Phase (Workflow Registration)

1. **outputId Uniqueness**: No two nodes in the same workflow may have duplicate `outputId` values
2. **Field Name Conflict Detection**: Warn if multiple nodes would produce overlapping field names
3. **outputId Format Validation**: Must match `/^[a-zA-Z_][a-zA-Z0-9_]*$/` pattern

### Subgraph/Child Execution Phase

1. **Name Collision Handling**: When subgraphs expand, their node `outputId`s need prefixing to avoid conflicts with parent workflow
2. **Scoped Prefixing**: Subgraph output IDs should be prefixed with subgraph node ID to ensure uniqueness

### Graph Validation Phase

1. **Edge Reference Validation**: Verify that condition expressions referencing node outputs actually point to existing nodes
2. **Field Existence Check**: If a condition references `node.llm_result.content`, verify that LLM node with that outputId exists

## Known Issues

### 1. Output Types Decoupled from Configs

Currently, output type interfaces (`LLMNodeOutput`, `RouteNodeOutput`, etc.) are defined independently in `node-outputs.ts`. This means:

- Changes to `LLMNodeConfig` won't produce compile errors in `LLMNodeOutput`
- Developers must remember to update both files
- The relationship is implicit, not enforced by the type system

**Suggested Fix**: Embed output definitions directly into node config interfaces.

```typescript
// Instead of:
export interface LLMNodeConfig { ... }       // execution-configs.ts
export interface LLMNodeOutput { ... }       // node-outputs.ts

// Do:
export interface LLMNodeConfig {
  profileId: ID;
  contextRefs?: string[];
  // ...
  /** Output fields - statically tied to this config */
  _output: {
    content: string;
    toolCalls?: Array<{ id: string; name: string; arguments: unknown }>;
  };
}
```

### 2. ADD_TOOL Node is Dead Code

- Has no runtime handler in `getNodeHandler()`
- `scope` field is deprecated per tool visibility refactoring
- Tool management is now handled by `TOOL_VISIBILITY` and `AvailableTools`

**Suggested Action**: Remove ADD_TOOL node type entirely.

### 3. Default Prompt Context Not Merged into 'current'

The `initializeExecutionContext` creates separate `current` (empty) and `system` (with systemMessages) contexts.
LLM nodes default to `contextRefs: ['current']`, meaning system messages are NOT automatically included.

**Suggested Fix**: Initialize `current` context with system messages, or auto-merge `system` into `current`.

## Appendix: RuntimeNodeOutputMap

The `RuntimeNodeOutputMap` provides type-safe mapping from node type to output type:

```typescript
interface RuntimeNodeOutputMap {
  START: StartNodeOutput;
  END: EndNodeOutput;
  VARIABLE: VariableNodeOutput;
  FORK: ForkNodeOutput;
  JOIN: JoinNodeOutput;
  SYNC: SyncNodeOutput;
  SUBGRAPH: SubgraphNodeOutput;
  SCRIPT: ScriptNodeOutput;
  LLM: LLMNodeOutput;
  ADD_TOOL: AddToolNodeOutput;
  TOOL_VISIBILITY: ToolVisibilityNodeOutput;
  USER_INTERACTION: UserInteractionNodeOutput;
  ROUTE: RouteNodeOutput;
  CONTEXT_PROCESSOR: ContextProcessorNodeOutput;
  LOOP_START: LoopStartNodeOutput;
  LOOP_END: LoopEndNodeOutput;
  AGENT_LOOP: AgentLoopNodeOutput;
  START_FROM_TRIGGER: StartFromTriggerNodeOutput;
  CONTINUE_FROM_TRIGGER: ContinueFromTriggerNodeOutput;
  EMBED_START: EmbedStartNodeOutput;
  EMBED_END: EmbedEndNodeOutput;
}

type RuntimeNodeOutputOfType<T extends RuntimeNodeType> = RuntimeNodeOutputMap[T];
```