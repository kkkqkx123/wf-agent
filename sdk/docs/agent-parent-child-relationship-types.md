# Agent Parent-Child Relationship Type Definitions

## Overview

This document describes the type definitions added to support parent-child relationship management between WorkflowExecution and AgentLoop execution instances.

## Architecture Pattern

Following the established workflow pattern, the SDK maintains a clear separation:

### Types Package (`@wf-agent/types`)
- **Pure data structures** (interfaces, no methods)
- **Serializable** to JSON/TOML
- Used for configuration, storage, events, and cross-module contracts

### SDK Layer
- **Runtime entities** with methods and DI-injected components
- Manage lifecycle, state transitions, and business logic
- Implement the contracts defined in types package

## Type Definitions Added

### 1. `AgentLoopParentContext` (NEW)

**Location**: `packages/types/src/agent-execution/context.ts`

```typescript
export interface AgentLoopParentContext {
  /** Parent Workflow Execution ID */
  parentExecutionId: ID;

  /** Node ID in parent workflow */
  nodeId?: ID;
}
```

**Purpose**: Defines the parent-child relationship structure between WorkflowExecution and AgentLoop instances.

**Usage**:
- Establishes bidirectional relationship contract
- Enables lifecycle management (cleanup when parent stops)
- Tracked by both `WorkflowExecutionEntity` and `AgentLoopRegistry`

### 2. Enhanced `AgentLoopExecution` Interface

**Location**: `packages/types/src/agent-execution/definition.ts`

Already existed, now with enhanced documentation:

```typescript
export interface AgentLoopExecution {
  // ... other fields ...
  
  /**
   * Parent Workflow Execution ID (if executed as Graph node)
   *
   * This field establishes the parent-child relationship between
   * WorkflowExecution and AgentLoop execution instances.
   *
   * When an AGENT_LOOP node is executed within a workflow:
   * - This field is set to the parent WorkflowExecution ID
   * - The parent WorkflowExecutionEntity tracks this AgentLoop via childAgentLoopIds
   * - Enables lifecycle management (cleanup when parent workflow stops)
   *
   * @see AgentLoopParentContext - Related context type for relationship management
   */
  parentWorkflowExecutionId?: ID;

  /**
   * Node ID in parent workflow (if executed as Graph node)
   *
   * Identifies which AGENT_LOOP node in the parent workflow triggered this execution.
   * Useful for debugging and tracing execution flow.
   */
  nodeId?: ID;
}
```

### 3. Enhanced `AgentLoopExecutionOptions` Interface

**Location**: `packages/types/src/agent-execution/context.ts`

Enhanced documentation for existing fields:

```typescript
export interface AgentLoopExecutionOptions {
  // ... other fields ...
  
  /**
   * Parent Workflow Execution ID (when executed as Graph node)
   *
   * Establishes parent-child relationship for lifecycle management.
   * When set, the AgentLoop will be registered with the parent WorkflowExecutionEntity
   * and cleaned up automatically when the parent workflow stops.
   *
   * @see AgentLoopParentContext - Type defining the parent-child relationship structure
   */
  parentExecutionId?: ID;

  /**
   * Node ID in parent workflow (when executed as Graph node)
   *
   * Identifies which AGENT_LOOP node triggered this execution.
   * Used for debugging and execution tracing.
   */
  nodeId?: ID;
}
```

### 4. Enhanced `AgentLoopExecutionContext` Interface

**Location**: `packages/types/src/agent-execution/context.ts`

Enhanced documentation for runtime context fields:

```typescript
export interface AgentLoopExecutionContext {
  // ... other fields ...
  
  /**
   * Parent execution ID (if executed as Graph node)
   *
   * Indicates this AgentLoop is a child of a WorkflowExecution.
   * Used for lifecycle management and execution tracing.
   */
  parentExecutionId?: ID;

  /**
   * Node ID in parent workflow (if executed as Graph node)
   *
   * Identifies the AGENT_LOOP node that triggered this execution.
   */
  nodeId?: ID;
}
```

## Relationship Management Flow

### Registration (Creation Time)

```typescript
// In AgentLoopFactory.create()
if (options.parentExecutionId) {
  await this.registerWithParentExecution(id, options.parentExecutionId);
}

private static async registerWithParentExecution(
  agentLoopId: string,
  parentExecutionId: string,
): Promise<void> {
  const executionRegistry = container.get(Identifiers.WorkflowExecutionRegistry);
  const executionEntity = executionRegistry.get(parentExecutionId);
  if (executionEntity) {
    // Bidirectional link:
    // 1. AgentLoopEntity.parentExecutionId = parentExecutionId (child → parent)
    // 2. WorkflowExecutionEntity.childAgentLoopIds.add(agentLoopId) (parent → children)
    executionEntity.registerChildAgentLoop(agentLoopId);
  }
}
```

### Cleanup (Lifecycle Management)

```typescript
// In WorkflowLifecycleCoordinator.stopWorkflowExecution()
async stopWorkflowExecution(executionId: string): Promise<void> {
  // ... cancel workflow ...
  
  // Cleanup child AgentLoops
  await this.cleanupChildAgentLoops(executionId);
}

private async cleanupChildAgentLoops(executionId: string): Promise<void> {
  const agentLoopRegistry = container.get(Identifiers.AgentLoopRegistry);
  const cleanedCount = agentLoopRegistry.cleanupByParentWorkflowExecutionId(executionId);
  // Stops and removes all AgentLoops with parentExecutionId === executionId
}
```

## Comparison with Workflow Pattern

| Aspect | Workflow Sub-workflows | Agent-Workflow |
|--------|----------------------|----------------|
| Parent Context Type | `TriggeredSubworkflowContext` | `AgentLoopParentContext` ✅ NEW |
| Data Object | `WorkflowExecution` | `AgentLoopExecution` ✅ ENHANCED |
| Entity Class | `WorkflowExecutionEntity` | `AgentLoopEntity` |
| Registry | `WorkflowExecutionRegistry` | `AgentLoopRegistry` |
| Child Tracking | `childExecutionIds: ID[]` | `childAgentLoopIds: Set<string>` |
| Cleanup Method | `cascadeCancel()` | `cleanupByParentWorkflowExecutionId()` |

## Benefits

1. **Type Safety**: Compile-time checking of parent-child relationships
2. **Documentation**: Clear contract definition for cross-module interactions
3. **Consistency**: Follows established workflow architecture pattern
4. **Discoverability**: IDE autocomplete for relationship-related fields
5. **Maintainability**: Single source of truth for relationship structure

## Export Chain

```
packages/types/src/agent-execution/context.ts
  ↓ (exports AgentLoopParentContext)
packages/types/src/agent-execution/index.ts
  ↓ (re-exports from context.ts)
packages/types/src/index.ts
  ↓ (re-exports from agent-execution/)
External consumers import from "@wf-agent/types"
```

## Usage Example

```typescript
import type { 
  AgentLoopParentContext,
  AgentLoopExecutionOptions 
} from "@wf-agent/types";

// Define parent context
const parentContext: AgentLoopParentContext = {
  parentExecutionId: "wfexec-123",
  nodeId: "agent-node-456"
};

// Use in execution options
const options: AgentLoopExecutionOptions = {
  parentExecutionId: parentContext.parentExecutionId,
  nodeId: parentContext.nodeId,
  initialMessages: [...]
};

// Create AgentLoop with parent relationship
const agentLoop = await AgentLoopFactory.create(config, options);
// agentLoop.parentExecutionId === "wfexec-123"
// agentLoop.nodeId === "agent-node-456"
```

## Files Modified

1. ✅ `packages/types/src/agent-execution/context.ts` - Added `AgentLoopParentContext`, enhanced docs
2. ✅ `packages/types/src/agent-execution/definition.ts` - Enhanced `AgentLoopExecution` docs
3. ✅ `packages/types/src/agent-execution/index.ts` - Exported `AgentLoopParentContext`

## No Breaking Changes

All modifications are additive:
- New interface added (`AgentLoopParentContext`)
- Existing interfaces enhanced with better documentation
- No changes to method signatures or behavior
- Backward compatible with existing code
