# Implementation Guide: Iteration Hierarchy & Variable Architecture

This guide provides step-by-step instructions for implementing the iteration hierarchy and scoped variable management system.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Phase 1: Core Infrastructure](#phase-1-core-infrastructure)
3. [Phase 2: Integration](#phase-2-integration)
4. [Phase 3: Testing](#phase-3-testing)
5. [Phase 4: Documentation](#phase-4-documentation)
6. [Common Issues](#common-issues)

---

## Prerequisites

### Required Knowledge

- TypeScript advanced types and interfaces
- State management patterns (StateManager interface)
- Checkpoint serialization/deserialization
- Event-driven architecture (hooks, emitters)
- Parent-child execution relationships

### Files to Review Before Starting

1. `sdk/agent/state-managers/variable-state.ts` - Current variable implementation
2. `sdk/agent/state-managers/agent-loop-state.ts` - Iteration tracking
3. `sdk/core/execution/execution-hierarchy-manager.ts` - Hierarchy pattern
4. `sdk/workflow/state-managers/variable-state.ts` - Workflow variable reference
5. `packages/types/src/agent-execution/types.ts` - Type definitions

---

## Phase 1: Core Infrastructure

### Step 1: Create IterationContextManager

**File:** `sdk/agent/state-managers/iteration-context-manager.ts`

```typescript
/**
 * Iteration Context Manager
 * 
 * Manages iteration-scoped state for Agent Loop executions.
 * Each iteration has isolated variables, messages, and tool call tracking.
 */

import type { LLMMessage } from '@wf-agent/types';
import { createContextualLogger } from '../../utils/contextual-logger.js';

const logger = createContextualLogger({ component: 'IterationContextManager' });

export interface IterationContext {
  iteration: number;
  startTime: number;
  endTime?: number;
  variables: Map<string, unknown>;
  messages: LLMMessage[];
  toolCallIds: string[];
  status: 'pending' | 'running' | 'completed' | 'failed';
  error?: unknown;
}

export class IterationContextManager {
  private contexts: Map<number, IterationContext> = new Map();
  private currentIteration: number = 0;
  private maxRetainedIterations: number = 10;
  
  constructor(private agentLoopId: string) {
    logger.debug('IterationContextManager created', { agentLoopId });
  }
  
  // Implement all methods from architecture design...
  // (See full implementation in architecture document)
}
```

**Key Implementation Notes:**
- Use `Map` for O(1) lookups
- Implement automatic cleanup in `enterIteration()`
- Always clone data in `createSnapshot()` to avoid mutation issues
- Log all state transitions for debugging

### Step 2: Update AgentLoopEntity

**File:** `sdk/agent/entities/agent-loop-entity.ts`

```typescript
import { IterationContextManager } from '../state-managers/iteration-context-manager.js';

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
    
    // Initialize iteration context manager
    this.iterationContextManager = new IterationContextManager(id);
  }
  
  // Add new methods for scoped variable access
  
  /**
   * Set variable with scope resolution
   */
  setScopedVariable(name: string, value: unknown): void {
    const currentCtx = this.iterationContextManager.getCurrentContext();
    
    if (currentCtx) {
      this.iterationContextManager.setVariable(name, value);
    } else {
      // Fallback to agent-loop scope for backward compatibility
      this.variableStateManager.setVariableValue(name, value, 'workflowExecution');
    }
  }
  
  /**
   * Get variable with scope resolution
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
  
  // ... rest of class ...
}
```

### Step 3: Update Exports

**File:** `sdk/agent/state-managers/index.ts`

```typescript
export { AgentLoopState } from "./agent-loop-state.js";
export { MessageHistory, type MessageHistoryState } from "./message-history.js";
export { VariableState, type VariableStateSnapshot } from "./variable-state.js";
export { IterationContextManager, type IterationContext } from "./iteration-context-manager.js"; // NEW
```

**File:** `sdk/agent/index.ts`

```typescript
// Add to exports
export {
  AgentLoopState,
  MessageHistory,
  VariableState,
  IterationContextManager,  // NEW
  type MessageHistoryState,
  type VariableStateSnapshot,
  type IterationContext,    // NEW
} from "./state-managers/index.js";
```

---

## Phase 2: Integration

### Step 4: Integrate with Execution Flow

**File:** `sdk/agent/execution/executors/agent-loop-executor.ts`

Locate the main execution loop and add iteration lifecycle calls:

```typescript
async *execute(entity: AgentLoopEntity): AsyncGenerator<AgentLoopStreamEvent> {
  while (entity.state.currentIteration < maxIterations) {
    // Enter iteration scope (NEW)
    entity.iterationContextManager.enterIteration(entity.state.currentIteration + 1);
    
    try {
      // ... existing execution logic ...
      
      // Record messages in iteration context (NEW)
      for (const message of newMessages) {
        entity.iterationContextManager.addMessage(message);
      }
      
      // Record tool calls in iteration context (NEW)
      for (const toolCall of toolCalls) {
        entity.iterationContextManager.recordToolCall(toolCall.id);
      }
      
      // Exit iteration successfully (NEW)
      entity.iterationContextManager.exitIteration();
      
    } catch (error) {
      // Exit iteration with error (NEW)
      entity.iterationContextManager.exitIteration(error);
      throw error;
    }
  }
}
```

### Step 5: Update Checkpoint Serialization

**File:** `sdk/agent/checkpoint/checkpoint-coordinator.ts`

Update snapshot creation:

```typescript
async createCheckpoint(entity: AgentLoopEntity): Promise<string> {
  const snapshot = {
    state: entity.state.createSnapshot(),
    messages: entity.conversationManager.createSnapshot(),
    variables: entity.variableStateManager.createSnapshot(),
    iterations: entity.iterationContextManager.createSnapshot(), // NEW
    timestamp: Date.now(),
  };
  
  // ... serialize and save ...
}
```

Update restoration:

```typescript
async restoreFromCheckpoint(
  entity: AgentLoopEntity,
  checkpointId: string
): Promise<void> {
  const snapshot = await this.loadCheckpoint(checkpointId);
  
  entity.state.restoreFromSnapshot(snapshot.state);
  entity.conversationManager.restoreFromSnapshot(snapshot.messages);
  entity.variableStateManager.restoreFromSnapshot(snapshot.variables);
  entity.iterationContextManager.restoreFromSnapshot(snapshot.iterations); // NEW
}
```

### Step 6: Add Deprecation Warnings

**File:** `sdk/agent/entities/agent-loop-entity.ts`

Add warnings to old API methods:

```typescript
import { sdkLogger as logger } from '../../utils/logger.js';

export class AgentLoopEntity {
  /**
   * @deprecated Use setScopedVariable() instead. Will be removed in v2.0.
   */
  setVariable(name: string, value: unknown): void {
    logger.warn(
      'setVariable is deprecated. Use setScopedVariable() for iteration-scoped variables.',
      { agentLoopId: this.id, variableName: name }
    );
    
    // Delegate to scoped version for backward compatibility
    this.setScopedVariable(name, value);
  }
  
  /**
   * @deprecated Use getScopedVariable() instead. Will be removed in v2.0.
   */
  getVariable(name: string): unknown {
    logger.warn(
      'getVariable is deprecated. Use getScopedVariable() for proper scope resolution.',
      { agentLoopId: this.id, variableName: name }
    );
    
    return this.getScopedVariable(name);
  }
}
```

---

## Phase 3: Testing

### Step 7: Unit Tests for IterationContextManager

**File:** `sdk/agent/state-managers/__tests__/iteration-context-manager.test.ts`

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import { IterationContextManager } from '../iteration-context-manager.js';

describe('IterationContextManager', () => {
  let manager: IterationContextManager;
  
  beforeEach(() => {
    manager = new IterationContextManager('test-agent-1');
  });
  
  it('should create isolated iteration contexts', () => {
    // Enter iteration 1
    manager.enterIteration(1);
    manager.setVariable('x', 1);
    
    // Enter iteration 2
    manager.enterIteration(2);
    manager.setVariable('x', 2);
    
    // Verify isolation
    expect(manager.getVariable('x')).toBe(2);
    
    // Go back to iteration 1
    manager.enterIteration(1);
    expect(manager.getVariable('x')).toBe(1);
  });
  
  it('should cleanup old iterations beyond retention limit', () => {
    // Create 15 iterations
    for (let i = 1; i <= 15; i++) {
      manager.enterIteration(i);
      manager.setVariable('data', i);
      manager.exitIteration();
    }
    
    // Should only retain last 10
    const history = manager.getIterationHistory();
    expect(history.length).toBeLessThanOrEqual(10);
    expect(history[0].iteration).toBeGreaterThan(1);
  });
  
  it('should create restorable snapshots', () => {
    manager.enterIteration(1);
    manager.setVariable('key', 'value');
    manager.addMessage({ role: 'user', content: 'test' });
    manager.exitIteration();
    
    const snapshot = manager.createSnapshot();
    
    // Create new manager and restore
    const restored = new IterationContextManager('test-agent-2');
    restored.restoreFromSnapshot(snapshot);
    
    expect(restored.getVariable('key')).toBe('value');
    expect(restored.getMessages()).toHaveLength(1);
  });
  
  it('should track tool calls per iteration', () => {
    manager.enterIteration(1);
    manager.recordToolCall('call_1');
    manager.recordToolCall('call_2');
    
    const context = manager.getCurrentContext();
    expect(context?.toolCallIds).toEqual(['call_1', 'call_2']);
  });
});
```

### Step 8: Integration Tests

**File:** `sdk/agent/__tests__/integration/iteration-scoping.test.ts`

```typescript
import { describe, it, expect } from 'vitest';
import { AgentLoopFactory } from '../execution/factories/agent-loop-factory.js';

describe('Iteration Scoping Integration', () => {
  it('should isolate variables across iterations', async () => {
    const entity = await AgentLoopFactory.create({
      profileId: 'test-profile',
      systemPrompt: 'Test agent',
    });
    
    // Iteration 1
    entity.iterationContextManager.enterIteration(1);
    entity.setScopedVariable('counter', 1);
    entity.iterationContextManager.exitIteration();
    
    // Iteration 2
    entity.iterationContextManager.enterIteration(2);
    entity.setScopedVariable('counter', 2);
    
    // Verify iteration 2 has its own value
    expect(entity.getScopedVariable('counter')).toBe(2);
    
    // Can still access iteration 1
    const iter1 = entity.iterationContextManager.getIterationContext(1);
    expect(iter1?.variables.get('counter')).toBe(1);
  });
  
  it('should persist important data to agent-loop scope', async () => {
    const entity = await AgentLoopFactory.create({
      profileId: 'test-profile',
      systemPrompt: 'Test agent',
    });
    
    // Set persistent variable (no active iteration)
    entity.variableStateManager.setVariableValue('persistent', 'data', 'workflowExecution');
    
    // Iteration can read persistent data
    entity.iterationContextManager.enterIteration(1);
    expect(entity.getScopedVariable('persistent')).toBe('data');
    
    // Iteration can override temporarily
    entity.setScopedVariable('persistent', 'temp');
    expect(entity.getScopedVariable('persistent')).toBe('temp');
    
    // After iteration, falls back to persistent value
    entity.iterationContextManager.exitIteration();
    expect(entity.getScopedVariable('persistent')).toBe('data');
  });
});
```

### Step 9: Run Tests

```bash
# Run unit tests
cd sdk
pnpm test agent/state-managers/__tests__/iteration-context-manager.test.ts

# Run integration tests
pnpm test agent/__tests__/integration/iteration-scoping.test.ts

# Run all agent tests
pnpm test agent/
```

---

## Phase 4: Documentation

### Step 10: Update API Documentation

**File:** `sdk/docs/agent/README.md` (create if doesn't exist)

```markdown
# Agent Loop API Documentation

## Variable Management

### Scoped Variables (Recommended)

```typescript
// Set variable in current iteration scope
entity.setScopedVariable('name', value);

// Get variable with scope resolution
const value = entity.getScopedVariable('name');
```

### Legacy API (Deprecated)

```typescript
// Deprecated - use setScopedVariable instead
entity.setVariable('name', value);

// Deprecated - use getScopedVariable instead  
entity.getVariable('name');
```

## Iteration Management

### Manual Iteration Control

```typescript
// Enter iteration scope
entity.iterationContextManager.enterIteration(1);

// Work with iteration-scoped state
entity.setScopedVariable('temp', data);

// Exit iteration
entity.iterationContextManager.exitIteration();
```

### Accessing Iteration History

```typescript
// Get all iteration contexts
const history = entity.iterationContextManager.getIterationHistory();

// Get specific iteration
const iteration1 = entity.iterationContextManager.getIterationContext(1);

// Access iteration variables
const vars = iteration1?.variables;
```
```

### Step 11: Add Code Examples

Create example files in `sdk/examples/agent/`:

**File:** `sdk/examples/agent/iteration-scoping-example.ts`

```typescript
/**
 * Example: Using iteration-scoped variables
 */

import { AgentLoopFactory } from '@wf-agent/sdk/agent';

async function example() {
  const entity = await AgentLoopFactory.create({
    profileId: 'example-agent',
    systemPrompt: 'You are a helpful assistant.',
  });
  
  // Simulate multi-turn conversation with iteration scoping
  for (let iteration = 1; iteration <= 3; iteration++) {
    console.log(`\n=== Iteration ${iteration} ===`);
    
    // Enter iteration scope
    entity.iterationContextManager.enterIteration(iteration);
    
    try {
      // Set iteration-specific data
      entity.setScopedVariable('user_input', `Question ${iteration}`);
      entity.setScopedVariable('timestamp', Date.now());
      
      // Access current iteration state
      const currentVars = entity.iterationContextManager.getAllVariables();
      console.log('Current iteration variables:', currentVars);
      
      // Access previous iteration (if exists)
      if (iteration > 1) {
        const prevIteration = entity.iterationContextManager.getIterationContext(iteration - 1);
        console.log('Previous iteration timestamp:', prevIteration?.variables.get('timestamp'));
      }
      
      // Simulate work...
      await new Promise(resolve => setTimeout(resolve, 100));
      
    } finally {
      // Always exit iteration
      entity.iterationContextManager.exitIteration();
    }
  }
  
  // View complete iteration history
  const history = entity.iterationContextManager.getIterationHistory();
  console.log('\nTotal iterations:', history.length);
}

example().catch(console.error);
```

---

## Common Issues

### Issue 1: TypeScript Compilation Errors

**Problem:** Type errors when using new APIs

**Solution:**
```bash
# Rebuild types package first
cd packages/types
pnpm build

# Then rebuild SDK
cd ../../sdk
pnpm build
```

### Issue 2: Iteration Context Not Found

**Problem:** `getCurrentContext()` returns `undefined`

**Cause:** Forgot to call `enterIteration()`

**Solution:**
```typescript
// Always enter iteration before using scoped variables
entity.iterationContextManager.enterIteration(n);
// ... use scoped variables ...
entity.iterationContextManager.exitIteration();
```

### Issue 3: Checkpoint Restoration Fails

**Problem:** Error when restoring from checkpoint

**Cause:** Snapshot format mismatch

**Solution:**
```typescript
// Ensure snapshot includes iterations field
const snapshot = {
  state: ...,
  messages: ...,
  variables: ...,
  iterations: entity.iterationContextManager.createSnapshot(), // Don't forget!
};
```

### Issue 4: Memory Leak

**Problem:** Memory usage grows unbounded

**Cause:** Not cleaning up completed iterations

**Solution:**
```typescript
// Adjust retention limit based on needs
class IterationContextManager {
  private maxRetainedIterations: number = 10; // Default
  
  // Or configure via constructor
  constructor(agentLoopId: string, maxRetained: number = 10) {
    this.maxRetainedIterations = maxRetained;
  }
}
```

---

## Validation Checklist

Before considering implementation complete:

- [ ] All unit tests pass
- [ ] Integration tests pass
- [ ] No TypeScript compilation errors
- [ ] Deprecation warnings appear for old APIs
- [ ] Checkpoint serialization includes iteration state
- [ ] Checkpoint restoration works correctly
- [ ] Memory cleanup works (test with 100+ iterations)
- [ ] Documentation updated
- [ ] Examples provided
- [ ] Migration guide written

---

## Next Steps

After completing implementation:

1. **Performance Testing**: Benchmark with large iteration counts
2. **User Feedback**: Gather feedback from early adopters
3. **Bug Fixes**: Address any issues found in testing
4. **Deprecation Timeline**: Set date for v2.0 breaking changes
5. **Community Communication**: Announce new features via release notes

---

## Support

For questions or issues:
- Review architecture document: `sdk/docs/agent/iteration-hierarchy-variable-architecture.md`
- Check quick reference: `sdk/docs/agent/iteration-hierarchy-quick-reference.md`
- File issue in project tracker
- Contact maintainers
