# Agent Loop Hierarchy & Variable Architecture Design

## Overview

This document presents a comprehensive architectural analysis and design for iteration hierarchy levels and variable management in sub-agent and multi-iteration scenarios within the Agent Loop system.

## Table of Contents

1. [Current Architecture Analysis](#current-architecture-analysis)
2. [Problem Statement](#problem-statement)
3. [Proposed Architecture](#proposed-architecture)
4. [Iteration as Independent Hierarchy Level](#iteration-as-independent-hierarchy-level)
5. [Variable Management Strategy](#variable-management-strategy)
6. [Implementation Roadmap](#implementation-roadmap)
7. [Migration Guide](#migration-guide)
8. [Design Decisions & Rationale](#design-decisions--rationale)

---

## Current Architecture Analysis

### Existing Hierarchy Model

The current system uses `ExecutionHierarchyManager` to manage parent-child relationships:

```typescript
interface ParentExecutionContext {
  parentType: 'WORKFLOW' | 'AGENT_LOOP';
  parentId: ID;
  nodeId?: ID;  // Only for WORKFLOW parent
}

interface ChildExecutionReference {
  childType: 'WORKFLOW' | 'AGENT_LOOP';
  childId: ID;
  createdAt: number;
}
```

**Current Hierarchy Levels:**
1. **Root Workflow Execution** (depth 0)
2. **Sub-workflow / Triggered Subgraph** (depth 1+)
3. **Agent Loop as Graph Node** (depth N, child of workflow)

**Missing Hierarchy Level:**
- ❌ **Iteration level** - Not represented in hierarchy
- ❌ **Sub-agent within iteration** - No dedicated scope

### Current Variable Management

**Workflow Variables** (`sdk/workflow/state-managers/variable-state.ts`):
- ✅ 4-level scope system: `global`, `workflowExecution`, `local`, `loop`
- ✅ Rich metadata: type, description, readonly flag
- ✅ Scope isolation with enter/exit operations
- ✅ Proper checkpoint serialization

**Agent Variables** (`sdk/agent/state-managers/variable-state.ts`):
- ⚠️ Simplified to single scope: `workflowExecution` only
- ⚠️ Simple `Record<string, unknown>` structure
- ⚠️ Loses metadata during serialization
- ⚠️ Inconsistent with workflow pattern

### Current Iteration Handling

**AgentLoopState** tracks iterations via:
```typescript
private _currentIteration: number = 0;
private _iterationHistory: IterationRecord[] = [];
```

**IterationRecord** contains:
```typescript
interface IterationRecord {
  iteration: number;
  startTime: number;
  endTime?: number;
  toolCalls: ToolCallRecord[];
  responseContent?: string;
}
```

**Key Observation:** Iterations are tracked as sequential records but NOT as hierarchical scopes with isolated state.

---

## Problem Statement

### Problem 1: Missing Iteration Hierarchy

**Scenario:** Multi-turn agent conversation with stateful iterations

```typescript
// Current limitation: All iterations share same variable scope
Iteration 1: setVariable("context", "user wants code review")
Iteration 2: setVariable("context", "now user wants refactoring")  // Overwrites!
Iteration 3: getVariable("context")  // Gets iteration 2's value, lost iteration 1
```

**Issues:**
- ❌ No way to access previous iteration's state
- ❌ Cannot maintain per-iteration isolated context
- ❌ Difficult to implement "undo" or "compare iterations"
- ❌ Checkpoint restores entire state, not iteration-specific snapshots

### Problem 2: Sub-Agent State Isolation

**Scenario:** Agent calls another agent (sub-agent) within an iteration

```typescript
// Parent Agent (Iteration 1)
setVariable("analysis_result", {...});

// Sub-Agent (called from parent iteration 1)
setVariable("analysis_result", {...});  // Overwrites parent's variable!

// Parent Agent resumes
getVariable("analysis_result")  // Gets sub-agent's result, not parent's!
```

**Issues:**
- ❌ Sub-agents pollute parent's variable namespace
- ❌ No automatic state synchronization between parent/child
- ❌ Manual state passing required (error-prone)
- ❌ No visibility into sub-agent's internal state from parent hooks

### Problem 3: Variable Scope Mismatch

**Workflow vs Agent inconsistency:**

| Feature | Workflow | Agent |
|---------|----------|-------|
| Scopes | 4 levels | 1 level |
| Metadata | Full | None |
| Isolation | Yes | No |
| Triggers | Yes | Via hooks only |
| Checkpoint | Complete | Partial |

**Impact:**
- Confusing API for developers
- Different mental models for similar concepts
- Harder to compose workflows with agents

---

## Proposed Architecture

### Vision

Transform the Agent Loop from a **flat execution model** to a **hierarchical stateful model** that mirrors the sophistication of Workflow execution while maintaining simplicity for linear iteration patterns.

### Core Principles

1. **Iteration as First-Class Citizen**: Each iteration has its own isolated scope
2. **Hierarchical Variable Resolution**: Parent → Iteration → Sub-agent scoping
3. **Automatic State Propagation**: Smart merging of parent/child states
4. **Backward Compatibility**: Gradual migration path for existing code
5. **Hook-Driven Observability**: Events for all state changes

---

## Iteration as Independent Hierarchy Level

### New Hierarchy Structure

```
Level 0: Root Workflow Execution
  └─ Level 1: Agent Loop Entity (as graph node or standalone)
       ├─ Level 2: Iteration #1
       │    ├─ Level 3: Sub-Agent A (optional)
       │    └─ Level 3: Sub-Agent B (optional)
       ├─ Level 2: Iteration #2
       │    └─ Level 3: Sub-Agent C
       └─ Level 2: Iteration #N
```

### Implementation Design

#### 1. Extend Execution Types

```typescript
// packages/types/src/execution/hierarchy.ts

export type ExecutionType = 
  | 'WORKFLOW' 
  | 'AGENT_LOOP' 
  | 'ITERATION'      // NEW
  | 'SUB_AGENT';     // NEW (alias for nested AGENT_LOOP)
```

#### 2. Iteration Context Manager

```typescript
// sdk/agent/state-managers/iteration-context-manager.ts

import type { LLMMessage } from '@wf-agent/types';
import { createContextualLogger } from '../../utils/contextual-logger.js';

const logger = createContextualLogger({ component: 'IterationContextManager' });

/**
 * Iteration-scoped state container
 */
export interface IterationContext {
  /** Iteration number (1-based) */
  iteration: number;
  
  /** Start timestamp */
  startTime: number;
  
  /** End timestamp */
  endTime?: number;
  
  /** Iteration-local variables (isolated from other iterations) */
  variables: Map<string, unknown>;
  
  /** Messages generated in this iteration */
  messages: LLMMessage[];
  
  /** Tool calls made in this iteration */
  toolCallIds: string[];
  
  /** Status */
  status: 'pending' | 'running' | 'completed' | 'failed';
  
  /** Error if failed */
  error?: unknown;
}

/**
 * Manages iteration-scoped contexts
 * 
 * Provides:
 * - Isolated variable storage per iteration
 * - Automatic cleanup of completed iterations (configurable)
 * - Access to current and historical iteration data
 */
export class IterationContextManager {
  private contexts: Map<number, IterationContext> = new Map();
  private currentIteration: number = 0;
  private maxRetainedIterations: number = 10;  // Configurable
  
  constructor(private agentLoopId: string) {
    logger.debug('IterationContextManager created', { agentLoopId });
  }
  
  /**
   * Enter a new iteration (creates isolated scope)
   */
  enterIteration(iteration: number): void {
    logger.debug('Entering iteration', { 
      agentLoopId: this.agentLoopId, 
      iteration 
    });
    
    const context: IterationContext = {
      iteration,
      startTime: Date.now(),
      variables: new Map(),
      messages: [],
      toolCallIds: [],
      status: 'running',
    };
    
    this.contexts.set(iteration, context);
    this.currentIteration = iteration;
    
    // Cleanup old iterations if exceeding retention limit
    this.cleanupOldIterations();
  }
  
  /**
   * Exit current iteration (marks as completed)
   */
  exitIteration(error?: unknown): void {
    const context = this.contexts.get(this.currentIteration);
    if (!context) {
      throw new Error(`No active iteration context for iteration ${this.currentIteration}`);
    }
    
    context.endTime = Date.now();
    context.status = error ? 'failed' : 'completed';
    if (error) {
      context.error = error;
    }
    
    logger.debug('Exited iteration', {
      agentLoopId: this.agentLoopId,
      iteration: this.currentIteration,
      duration: context.endTime - context.startTime,
      status: context.status,
    });
  }
  
  /**
   * Get current iteration context
   */
  getCurrentContext(): IterationContext | undefined {
    return this.contexts.get(this.currentIteration);
  }
  
  /**
   * Get context for specific iteration
   */
  getIterationContext(iteration: number): IterationContext | undefined {
    return this.contexts.get(iteration);
  }
  
  /**
   * Set variable in current iteration scope
   */
  setVariable(name: string, value: unknown): void {
    const context = this.getCurrentContext();
    if (!context) {
      throw new Error('No active iteration context');
    }
    
    context.variables.set(name, value);
    logger.debug('Set iteration variable', {
      agentLoopId: this.agentLoopId,
      iteration: context.iteration,
      variableName: name,
    });
  }
  
  /**
   * Get variable from current iteration scope
   * Falls back to parent scopes if not found
   */
  getVariable(name: string): unknown {
    const context = this.getCurrentContext();
    if (!context) {
      return undefined;
    }
    
    // Check current iteration first
    if (context.variables.has(name)) {
      return context.variables.get(name);
    }
    
    // TODO: Fall back to parent agent-loop scope
    // This requires integration with VariableStateManager
    
    return undefined;
  }
  
  /**
   * Get all variables from current iteration
   */
  getAllVariables(): Record<string, unknown> {
    const context = this.getCurrentContext();
    if (!context) {
      return {};
    }
    
    return Object.fromEntries(context.variables);
  }
  
  /**
   * Add message to current iteration
   */
  addMessage(message: LLMMessage): void {
    const context = this.getCurrentContext();
    if (!context) {
      throw new Error('No active iteration context');
    }
    
    context.messages.push(message);
  }
  
  /**
   * Get messages from current iteration
   */
  getMessages(): LLMMessage[] {
    const context = this.getCurrentContext();
    return context ? [...context.messages] : [];
  }
  
  /**
   * Record tool call in current iteration
   */
  recordToolCall(toolCallId: string): void {
    const context = this.getCurrentContext();
    if (!context) {
      throw new Error('No active iteration context');
    }
    
    context.toolCallIds.push(toolCallId);
  }
  
  /**
   * Get iteration history
   */
  getIterationHistory(): IterationContext[] {
    return Array.from(this.contexts.values())
      .sort((a, b) => a.iteration - b.iteration);
  }
  
  /**
   * Create snapshot for checkpointing
   */
  createSnapshot(): {
    currentIteration: number;
    contexts: Array<{
      iteration: number;
      startTime: number;
      endTime?: number;
      variables: Record<string, unknown>;
      messages: LLMMessage[];
      toolCallIds: string[];
      status: string;
      error?: unknown;
    }>;
  } {
    return {
      currentIteration: this.currentIteration,
      contexts: Array.from(this.contexts.values()).map(ctx => ({
        iteration: ctx.iteration,
        startTime: ctx.startTime,
        endTime: ctx.endTime,
        variables: Object.fromEntries(ctx.variables),
        messages: [...ctx.messages],
        toolCallIds: [...ctx.toolCallIds],
        status: ctx.status,
        error: ctx.error,
      })),
    };
  }
  
  /**
   * Restore from snapshot
   */
  restoreFromSnapshot(snapshot: ReturnType<IterationContextManager['createSnapshot']>): void {
    this.currentIteration = snapshot.currentIteration;
    this.contexts.clear();
    
    for (const ctxData of snapshot.contexts) {
      const context: IterationContext = {
        iteration: ctxData.iteration,
        startTime: ctxData.startTime,
        endTime: ctxData.endTime,
        variables: new Map(Object.entries(ctxData.variables)),
        messages: ctxData.messages,
        toolCallIds: ctxData.toolCallIds,
        status: ctxData.status as any,
        error: ctxData.error,
      };
      
      this.contexts.set(ctxData.iteration, context);
    }
  }
  
  /**
   * Cleanup old iterations beyond retention limit
   */
  private cleanupOldIterations(): void {
    const iterations = Array.from(this.contexts.keys()).sort((a, b) => a - b);
    
    if (iterations.length <= this.maxRetainedIterations) {
      return;
    }
    
    const toRemove = iterations.slice(0, iterations.length - this.maxRetainedIterations);
    
    for (const iteration of toRemove) {
      const context = this.contexts.get(iteration);
      if (context?.status === 'completed') {
        logger.debug('Cleaning up completed iteration', {
          agentLoopId: this.agentLoopId,
          iteration,
        });
        this.contexts.delete(iteration);
      }
    }
  }
  
  /**
   * Cleanup all resources
   */
  cleanup(): void {
    this.contexts.clear();
    this.currentIteration = 0;
  }
}
```

#### 3. Integration with AgentLoopEntity

```typescript
// sdk/agent/entities/agent-loop-entity.ts (modifications)

export class AgentLoopEntity {
  // ... existing fields ...
  
  /** Iteration Context Manager (NEW) */
  readonly iterationContextManager: IterationContextManager;
  
  constructor(
    id: string,
    config: AgentLoopRuntimeConfig,
    state: AgentLoopState,
  ) {
    // ... existing initialization ...
    
    this.iterationContextManager = new IterationContextManager(id);
  }
  
  // NEW: Iteration-scoped variable operations
  
  /**
   * Set variable in current iteration scope
   * Falls back to agent-loop scope if no active iteration
   */
  setScopedVariable(name: string, value: unknown): void {
    const currentCtx = this.iterationContextManager.getCurrentContext();
    
    if (currentCtx) {
      // Set in iteration scope
      this.iterationContextManager.setVariable(name, value);
    } else {
      // Fallback to agent-loop scope (backward compatibility)
      this.variableStateManager.setVariableValue(name, value, 'workflowExecution');
    }
  }
  
  /**
   * Get variable with scope resolution
   * Priority: iteration > agent-loop > parent workflow
   */
  getScopedVariable(name: string): unknown {
    // Try iteration scope first
    const iterationValue = this.iterationContextManager.getVariable(name);
    if (iterationValue !== undefined) {
      return iterationValue;
    }
    
    // Fallback to agent-loop scope
    return this.variableStateManager.getVariableValue(name, 'workflowExecution');
  }
  
  /**
   * Get iteration-specific messages
   */
  getIterationMessages(): LLMMessage[] {
    return this.iterationContextManager.getMessages();
  }
}
```

---

## Variable Management Strategy

### Three-Tier Variable System

```
┌─────────────────────────────────────────────┐
│ Tier 1: Workflow-Level Variables            │
│ - Global scope (shared across executions)   │
│ - Managed by WorkflowExecutionEntity        │
└─────────────────────────────────────────────┘
                    ↓ inherits
┌─────────────────────────────────────────────┐
│ Tier 2: Agent-Loop-Level Variables          │
│ - Persistent across iterations              │
│ - Shared with sub-agents (read-only)        │
│ - Managed by VariableStateManager           │
└─────────────────────────────────────────────┘
                    ↓ isolates
┌─────────────────────────────────────────────┐
│ Tier 3: Iteration-Level Variables           │
│ - Isolated per iteration                    │
│ - Auto-cleanup after completion             │
│ - Managed by IterationContextManager        │
└─────────────────────────────────────────────┘
                    ↓ creates
┌─────────────────────────────────────────────┐
│ Tier 4: Sub-Agent Variables                 │
│ - Isolated per sub-agent instance           │
│ - Synchronized back to parent on completion │
│ - Managed by child AgentLoopEntity          │
└─────────────────────────────────────────────┘
```

### Variable Resolution Algorithm

```typescript
/**
 * Variable resolution follows this priority order:
 * 
 * 1. Current iteration scope (writeable)
 * 2. Agent-loop scope (writeable)
 * 3. Parent workflow scope (read-only)
 * 4. Global scope (read-only)
 * 
 * Write operations always go to the highest-priority writeable scope.
 */
function resolveVariable(
  entity: AgentLoopEntity,
  name: string,
  operation: 'read' | 'write',
  value?: unknown
): unknown {
  // WRITE operation
  if (operation === 'write') {
    const currentIteration = entity.iterationContextManager.getCurrentContext();
    
    if (currentIteration) {
      // Write to iteration scope (highest priority writeable)
      entity.iterationContextManager.setVariable(name, value);
      return value;
    } else {
      // No active iteration, write to agent-loop scope
      entity.variableStateManager.setVariableValue(name, value, 'workflowExecution');
      return value;
    }
  }
  
  // READ operation - follow resolution chain
  let result: unknown;
  
  // 1. Try iteration scope
  result = entity.iterationContextManager.getVariable(name);
  if (result !== undefined) {
    return result;
  }
  
  // 2. Try agent-loop scope
  result = entity.variableStateManager.getVariableValue(name, 'workflowExecution');
  if (result !== undefined) {
    return result;
  }
  
  // 3. Try parent workflow scope (if exists)
  const parentContext = entity.getParentContext();
  if (parentContext && parentContext.parentType === 'WORKFLOW') {
    const parentEntity = getParentWorkflowEntity(parentContext.parentId);
    if (parentEntity) {
      result = parentEntity.getVariable(name);
      if (result !== undefined) {
        return result;
      }
    }
  }
  
  // 4. Not found
  return undefined;
}
```

### Sub-Agent State Synchronization

#### Automatic Sync on Completion

```typescript
// sdk/agent/execution/handlers/sub-agent-sync.ts

import type { AgentLoopEntity } from '../../entities/agent-loop-entity.js';

/**
 * Synchronize sub-agent results back to parent
 * 
 * Called when sub-agent completes execution
 */
export async function syncSubAgentResults(
  parentEntity: AgentLoopEntity,
  subAgentEntity: AgentLoopEntity,
  syncStrategy: 'merge' | 'replace' | 'isolate' = 'merge'
): Promise<void> {
  const subAgentVariables = subAgentEntity.getAllVariables();
  const subAgentMessages = subAgentEntity.getMessages();
  
  switch (syncStrategy) {
    case 'merge':
      // Merge sub-agent variables into parent's current iteration
      for (const [key, value] of Object.entries(subAgentVariables)) {
        // Skip internal/system variables
        if (key.startsWith('_') || key.startsWith('$')) {
          continue;
        }
        
        // Prefix with sub-agent ID to avoid collisions
        const prefixedKey = `$subagent.${subAgentEntity.id}.${key}`;
        parentEntity.setScopedVariable(prefixedKey, value);
      }
      
      // Append sub-agent messages to parent's message history
      for (const message of subAgentMessages) {
        parentEntity.addMessage({
          ...message,
          metadata: {
            ...message.metadata,
            sourceAgentId: subAgentEntity.id,
            syncedFromSubAgent: true,
          },
        });
      }
      break;
      
    case 'replace':
      // Replace parent's variables with sub-agent's (dangerous!)
      parentEntity.variableStateManager.cleanup();
      for (const [key, value] of Object.entries(subAgentVariables)) {
        parentEntity.setScopedVariable(key, value);
      }
      break;
      
    case 'isolate':
      // Keep completely separate (default for safety)
      // Results accessible via event callbacks only
      break;
  }
  
  // Emit synchronization event
  await emit(parentEntity.eventManager, buildSubAgentSyncedEvent({
    parentAgentId: parentEntity.id,
    subAgentId: subAgentEntity.id,
    syncedVariables: Object.keys(subAgentVariables),
    syncedMessageCount: subAgentMessages.length,
    strategy: syncStrategy,
  }));
}
```

#### Hook-Based Variable Change Notifications

```typescript
// Example: Using hooks to react to variable changes

const config: AgentLoopRuntimeConfig = {
  profileId: 'my-agent',
  tools: [...],
  hooks: [
    {
      hookType: 'AFTER_ITERATION',
      eventName: 'iteration.state.captured',
      handler: async (context) => {
        // Access iteration-scoped variables
        const iterationVars = context.entity.iterationContextManager.getAllVariables();
        
        // Log or persist iteration state
        console.log(`Iteration ${context.iteration} variables:`, iterationVars);
        
        // Can also access previous iterations
        const prevIteration = context.entity.iterationContextManager.getIterationContext(
          context.iteration - 1
        );
        if (prevIteration) {
          console.log('Previous iteration vars:', prevIteration.variables);
        }
      },
    },
    {
      hookType: 'AFTER_TOOL_CALL',
      condition: 'toolCall.name === "call_sub_agent"',
      handler: async (context) => {
        // React to sub-agent completion
        const subAgentResult = context.toolCall?.result;
        
        // Sub-agent variables automatically synced if configured
        const syncedVars = context.entity.getScopedVariable('$subagent.*');
        
        // Use sub-agent results in next iteration
        context.entity.addSteeringMessage({
          role: 'user',
          content: `Sub-agent analysis complete: ${JSON.stringify(subAgentResult)}`,
        });
      },
    },
  ],
};
```

---

## Implementation Roadmap

### Phase 1: Foundation (Week 1-2)

**Goals:** Establish iteration context infrastructure without breaking existing functionality

**Tasks:**
1. ✅ Create `IterationContextManager` class
2. ✅ Add iteration context to `AgentLoopEntity`
3. ✅ Implement basic enter/exit iteration lifecycle
4. ✅ Add iteration-scoped variable storage
5. ✅ Write unit tests for iteration context

**Deliverables:**
- `sdk/agent/state-managers/iteration-context-manager.ts`
- Updated `AgentLoopEntity` with iteration manager
- Unit test suite

### Phase 2: Integration (Week 3-4)

**Goals:** Integrate iteration context with existing variable system

**Tasks:**
1. Implement scoped variable resolution (iteration → agent-loop → parent)
2. Update `setVariable`/`getVariable` to use scoped resolution
3. Add iteration context to checkpoint serialization
4. Implement iteration cleanup logic
5. Add observability hooks for iteration events

**Deliverables:**
- Scoped variable resolution algorithm
- Checkpoint support for iteration state
- Event emission for iteration lifecycle

### Phase 3: Sub-Agent Support (Week 5-6)

**Goals:** Enable proper state isolation and synchronization for sub-agents

**Tasks:**
1. Implement sub-agent variable sync strategies
2. Add automatic sync on sub-agent completion
3. Create hook handlers for sub-agent events
4. Document sub-agent best practices
5. Add integration tests for parent-child scenarios

**Deliverables:**
- Sub-agent synchronization module
- Hook handlers for sub-agent lifecycle
- Integration test suite

### Phase 4: Migration & Deprecation (Week 7-8)

**Goals:** Provide smooth migration path and deprecate old patterns

**Tasks:**
1. Mark flat `setVariable`/`getVariable` as deprecated
2. Add migration warnings in documentation
3. Create migration guide with examples
4. Update all internal usage to scoped APIs
5. Release v2.0 with breaking changes (if needed)

**Deliverables:**
- Deprecation notices in code
- Comprehensive migration guide
- Updated examples and tutorials

---

## Migration Guide

### For Application Developers

#### Before (Current API)

```typescript
// Old: Flat variable management
entity.setVariable('counter', 5);
const counter = entity.getVariable('counter');

// Problem: All iterations share same variable
// Iteration 1: counter = 5
// Iteration 2: counter = 10 (overwrites!)
```

#### After (New API)

```typescript
// New: Scoped variable management

// Option 1: Explicit scoped API (recommended)
entity.setScopedVariable('counter', 5);  // Writes to current iteration
const counter = entity.getScopedVariable('counter');  // Reads with fallback

// Option 2: Iteration context directly
entity.iterationContextManager.enterIteration(1);
entity.iterationContextManager.setVariable('counter', 5);
const iterationCounter = entity.iterationContextManager.getVariable('counter');

// Benefit: Each iteration has isolated state
// Iteration 1: counter = 5
// Iteration 2: counter = 10 (separate from iteration 1)
// Can still access iteration 1: getIterationContext(1).variables.get('counter')
```

#### Backward Compatibility

```typescript
// During transition period, old API still works
entity.setVariable('legacy_var', 'value');

// Internally resolves to:
// - If in active iteration → writes to iteration scope
// - Otherwise → writes to agent-loop scope (old behavior)

// Warning logged:
// "setVariable is deprecated, use setScopedVariable instead"
```

### For Framework Maintainers

#### Updating Checkpoint Logic

```typescript
// OLD: Only serialize AgentLoopState
const snapshot = {
  state: entity.state.createSnapshot(),
  messages: entity.conversationManager.createSnapshot(),
  variables: entity.variableStateManager.createSnapshot(),
};

// NEW: Also serialize iteration contexts
const snapshot = {
  state: entity.state.createSnapshot(),
  messages: entity.conversationManager.createSnapshot(),
  variables: entity.variableStateManager.createSnapshot(),
  iterations: entity.iterationContextManager.createSnapshot(),  // ADD THIS
};
```

#### Updating Hook Contexts

```typescript
// OLD: Hook context has flat variables
interface AgentHookEvaluationContext {
  variables: Record<string, unknown>;  // All variables merged
}

// NEW: Hook context includes iteration info
interface AgentHookEvaluationContext {
  variables: Record<string, unknown>;  // Still merged for convenience
  iteration: {
    current: number;
    context: IterationContext;
    history: IterationContext[];
  };
  getScopedVariable: (name: string) => unknown;  // NEW helper
}
```

---

## Design Decisions & Rationale

### Decision 1: Iteration as Separate Manager vs. Extending VariableState

**Chosen:** Separate `IterationContextManager`

**Rationale:**
- ✅ Clear separation of concerns (iteration lifecycle ≠ variable storage)
- ✅ Easier to test and maintain independently
- ✅ Can evolve iteration features without affecting variable system
- ❌ More classes to manage
- ❌ Requires coordination between managers

**Rejected Alternative:** Extend `VariableState` with iteration scopes
- Would couple iteration logic too tightly with variable implementation
- Makes VariableState responsible for too many concerns
- Harder to add iteration-specific features (messages, tool calls, etc.)

### Decision 2: Automatic vs. Manual Sub-Agent Sync

**Chosen:** Configurable automatic sync with opt-out

**Rationale:**
- ✅ Reduces boilerplate for common cases
- ✅ Prevents accidental state loss
- ✅ Configurable strategies provide flexibility
- ❌ May sync unnecessary data
- ❌ Adds complexity to sub-agent lifecycle

**Alternative Considered:** Manual sync only
- Too error-prone for developers
- Easy to forget synchronization
- Leads to subtle bugs

### Decision 3: Variable Resolution Priority

**Chosen:** Iteration > Agent-Loop > Parent Workflow > Global

**Rationale:**
- ✅ Most specific scope takes precedence (principle of least surprise)
- ✅ Allows iteration to override persistent state temporarily
- ✅ Maintains backward compatibility (agent-loop still accessible)
- ❌ May be confusing which scope a variable comes from
- ✅ Mitigated by debug logging and observability hooks

**Alternative Considered:** Agent-Loop > Iteration
- Would make iteration overrides harder
- Less intuitive for temporary state
- Breaks encapsulation principle

### Decision 4: Retention Policy for Completed Iterations

**Chosen:** Configurable retention with default of 10 iterations

**Rationale:**
- ✅ Balances memory usage with debugging capability
- ✅ Allows inspection of recent iteration history
- ✅ Configurable for different use cases
- ❌ May lose historical data if not persisted externally
- ✅ Mitigated by checkpoint mechanism for long-term storage

**Alternative Considered:** Keep all iterations indefinitely
- Memory leak risk for long-running agents
- Most applications don't need full history
- Better to persist externally if needed

### Decision 5: Breaking Changes vs. Backward Compatibility

**Chosen:** Gradual deprecation with backward compatibility layer

**Rationale:**
- ✅ Minimizes disruption to existing users
- ✅ Provides migration path
- ✅ Allows testing new APIs alongside old ones
- ❌ Maintains two code paths temporarily
- ❌ Slower adoption of new patterns

**Alternative Considered:** Immediate breaking change
- Would break all existing integrations
- High migration cost
- Damages developer trust

---

## Benefits Summary

### For Developers

1. **Clearer Mental Model**: Iterations as isolated scopes match intuitive understanding
2. **Easier Debugging**: Can inspect state at any iteration point
3. **Better Encapsulation**: Sub-agents don't pollute parent state
4. **Flexible State Management**: Choose appropriate scope for each variable

### For System Reliability

1. **Prevents State Corruption**: Isolated scopes prevent accidental overwrites
2. **Improved Checkpoint Accuracy**: Captures iteration-specific state
3. **Better Resource Management**: Automatic cleanup of completed iterations
4. **Predictable Behavior**: Clear variable resolution rules

### For Extensibility

1. **Hook Integration**: Rich events for iteration lifecycle
2. **Custom Strategies**: Pluggable sync strategies for sub-agents
3. **Future-Proof**: Architecture supports advanced features (time travel, comparison, etc.)

---

## Conclusion

This architecture transforms the Agent Loop from a simple iterative executor into a sophisticated stateful system with proper hierarchical scoping. By treating iterations as first-class citizens and implementing intelligent variable resolution, we enable complex multi-turn conversations and sub-agent orchestration while maintaining backward compatibility and developer ergonomics.

The phased implementation approach ensures minimal disruption while progressively unlocking powerful new capabilities for building robust, stateful agent systems.

---

## Appendix: Comparison with Workflow Architecture

| Aspect | Workflow | Agent Loop (Proposed) |
|--------|----------|----------------------|
| **Hierarchy** | Graph nodes + subgraphs | Iterations + sub-agents |
| **Scopes** | 4 levels (global/workflow/local/loop) | 4 levels (global/agent-loop/iteration/sub-agent) |
| **Parallelism** | Fork/join nodes | Concurrent sub-agents |
| **State Sync** | Explicit merge nodes | Automatic on completion |
| **Checkpoint** | Node-level granularity | Iteration-level granularity |
| **Use Case** | Complex workflows, parallel tasks | Conversational agents, iterative refinement |

Both architectures now share similar principles while optimized for their respective domains.
