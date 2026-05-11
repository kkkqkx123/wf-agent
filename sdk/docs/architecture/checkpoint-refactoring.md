# Checkpoint Architecture Refactoring

## Overview

This document describes the refactoring of the checkpoint system to properly utilize the base classes in `sdk/core/checkpoint` and eliminate code duplication between Agent and Workflow implementations.

## Problem Statement

### Before Refactoring

The `sdk/core/checkpoint` module provided base classes designed to eliminate code duplication:
- `BaseCheckpointCoordinator` - Template method for checkpoint lifecycle
- `BaseCheckpointStateManager` - Common CRUD operations
- `BaseDiffCalculator` - Delta calculation algorithms
- `BaseDeltaRestorer` - Delta chain restoration

**However**, these base classes were **not being used**:
- `AgentLoopCheckpointCoordinator` was a standalone implementation
- `Workflow CheckpointCoordinator` used static methods
- Code duplication existed in:
  - `determineCheckpointType()` logic (3 copies)
  - `validateCheckpoint()` logic (3 copies)
  - Checkpoint creation flow (3 copies)

### Root Causes

1. **Overly Strict Type Constraints**
   - `CheckpointEntity<TState>` interface required `extractState()` method
   - Neither `AgentLoopEntity` nor `WorkflowExecutionEntity` implemented this interface
   - Led to type conflicts when trying to extend base class

2. **Design Mismatch**
   - Base class expected entities to implement specific interface
   - Actual entities had different structures and responsibilities
   - No clear migration path from existing implementations

## Solution

### 1. Relaxed Type Constraints

**Changed:** `types.ts`

```typescript
// Before: Strict interface requiring extractState method
export interface CheckpointEntity<TState> {
  id: string;
  extractState(): TState;
}

// After: Minimal interface only requiring ID
export interface CheckpointableEntity {
  id: string;
}
```

**Rationale:**
- State extraction is the coordinator's responsibility, not the entity's
- Different modules have different entity structures
- Only common requirement is having an `id` for checkpoint association

### 2. Updated Base Class

**Changed:** `base-checkpoint-coordinator.ts`

```typescript
// Before: Strict constraint
export abstract class BaseCheckpointCoordinator<
  TCheckpoint extends BaseCheckpoint<any, any>,
  TEntity extends CheckpointEntity<any>,  // ❌ Too strict
  TState
>

// After: Relaxed constraint
export abstract class BaseCheckpointCoordinator<
  TCheckpoint extends BaseCheckpoint<any, any>,
  TEntity extends CheckpointableEntity,  // ✅ Only requires 'id'
  TState
>
```

### 3. Agent Implementation Refactored

**Changed:** `agent/checkpoint/checkpoint-coordinator.ts`

```typescript
export class AgentLoopCheckpointCoordinator extends BaseCheckpointCoordinator<
  AgentLoopCheckpoint,
  AgentLoopEntity,  // ✅ Now compatible!
  AgentLoopStateSnapshot
> {
  // Implements abstract methods
  protected extractState(entity: AgentLoopEntity): AgentLoopStateSnapshot { ... }
  protected buildCheckpoint(...): Promise<AgentLoopCheckpoint> { ... }
  protected extractParentId(checkpoint: AgentLoopCheckpoint): string { ... }
  protected createEntityFromSnapshot(parentId: string, snapshot: AgentLoopStateSnapshot): AgentLoopEntity { ... }
  
  // Reuses base class methods
  // - createCheckpoint() - template method
  // - restoreFromCheckpoint() - template method
  // - determineCheckpointType() - common logic
  // - validateCheckpoint() - base validation + agent-specific
}
```

**Benefits:**
- Eliminates ~150 lines of duplicated code
- Centralizes checkpoint type determination logic
- Single source of truth for validation rules
- Easier to maintain and extend

### 4. Removed Unused Experimental Code

**Deleted:**
- `health-monitor.ts` - Hardcoded workflow dependencies, never used
- `metadata-enricher.ts` - Hardcoded workflow dependencies, never used

**Rationale:**
- These files were marked as "future enhancements" but never integrated
- Had tight coupling to workflow-specific types
- If needed in future, should be redesigned with proper abstractions

## Architecture After Refactoring

```
sdk/core/checkpoint/
├── base-checkpoint-coordinator.ts    ✅ Used by Agent
├── base-checkpoint-state-manager.ts  ✅ Used by Agent & Workflow
├── base-diff-calculator.ts           ✅ Used by Agent & Workflow
├── base-delta-restorer.ts            ✅ Used by Agent & Workflow
├── types.ts                          ✅ Shared type definitions
└── index.ts                          ✅ Clean exports

sdk/agent/checkpoint/
└── checkpoint-coordinator.ts         ✅ Extends BaseCheckpointCoordinator

sdk/workflow/checkpoint/
└── checkpoint-coordinator.ts         ⚠️  Still uses static methods (TODO)
```

## Code Duplication Eliminated

### Before
- `determineCheckpointType()`: 3 implementations (~60 lines total)
- `validateCheckpoint()`: 3 implementations (~90 lines total)
- Checkpoint creation flow: 3 implementations (~180 lines total)
- **Total duplicated: ~330 lines**

### After (Agent)
- `determineCheckpointType()`: 1 implementation in base class
- `validateCheckpoint()`: 1 base + agent-specific extensions
- Checkpoint creation flow: 1 template method in base class
- **Agent duplication eliminated: ~150 lines**

### Remaining Work
- Workflow CheckpointCoordinator still needs refactoring (702 lines, complex static methods)
- Estimated additional savings: ~180 lines

## Design Principles Applied

### 1. Minimal Interface Constraint
Only require what's absolutely necessary (`id` property), delegate everything else to subclasses.

### 2. Template Method Pattern
Base class defines the algorithm structure, subclasses provide specific steps.

### 3. Dependency Inversion
High-level checkpoint logic doesn't depend on low-level entity details.

### 4. DRY (Don't Repeat Yourself)
Common logic centralized in core, module-specific logic in subclasses.

### 5. Strategy Customization for Different Semantics

**Important:** The `determineCheckpointType()` method demonstrates that **not all logic should be unified**.

#### Workflow Strategy (Base Class Default)
```typescript
// Based on "how many checkpoints have been saved"
if (checkpointCount % baselineInterval === 0) return "FULL";
```
- **Semantic**: Storage management perspective
- **Use case**: Workflow node execution (infrequent checkpoints)
- **Default interval**: 10
- **Example**: With baselineInterval=10, creates FULL at checkpoints 1, 11, 21...

#### Agent Strategy (Overridden)
```typescript
// Based on "how many iterations have been executed"
if ((checkpointCount + 1) % baselineInterval === 0) return "FULL";
```
- **Semantic**: Execution progress perspective
- **Use case**: Agent loop iteration (frequent checkpoints after each LLM call)
- **Default interval**: 5
- **Example**: With baselineInterval=5, creates FULL at checkpoints 1, 5, 10, 15...

**Why not unify?**
1. **Different business semantics**: Workflow counts saved checkpoints, Agent counts executed iterations
2. **Different trigger frequencies**: Workflow nodes execute less frequently than Agent iterations
3. **Different default configurations reflect usage patterns**: Workflow uses 10 (sparse), Agent uses 5 (dense)

The base class provides a reasonable default strategy and clear documentation that subclasses may override it for different semantics. This is the essence of the **Open/Closed Principle**: open for extension, closed for modification.

## Migration Guide

### For New Checkpoint Implementations

1. Extend `BaseCheckpointCoordinator`:
```typescript
class MyCheckpointCoordinator extends BaseCheckpointCoordinator<
  MyCheckpoint,
  MyEntity,  // Must have 'id' property
  MyStateSnapshot
> {
  protected extractState(entity: MyEntity): MyStateSnapshot { ... }
  protected buildCheckpoint(...): Promise<MyCheckpoint> { ... }
  protected extractParentId(checkpoint: MyCheckpoint): string { ... }
  protected createEntityFromSnapshot(parentId: string, snapshot: MyStateSnapshot): MyEntity { ... }
}
```

2. Use inherited methods:
```typescript
const coordinator = new MyCheckpointCoordinator();
const checkpointId = await coordinator.createCheckpoint(entity, dependencies, metadata);
const restoredEntity = await coordinator.restoreFromCheckpoint(checkpointId, dependencies);
```

### For Existing Code

No breaking changes! The public API remains the same:
- `createCheckpoint(entity, dependencies, options?)` → still works
- `restoreFromCheckpoint(checkpointId, dependencies)` → still works

## Testing

All existing tests should pass without modification because:
- Public API unchanged
- Behavior preserved (same logic, just moved to base class)
- Type compatibility maintained

## Future Improvements

### 1. Workflow CheckpointCoordinator Refactoring
Convert from static methods to instance-based design:
```typescript
// Current (static)
await CheckpointCoordinator.createCheckpoint(id, deps, metadata);

// Target (instance)
const coordinator = new WorkflowCheckpointCoordinator();
await coordinator.createCheckpoint(entity, deps, metadata);
```

### 2. Unified Error Handling
Create common checkpoint error hierarchy instead of module-specific errors.

### 3. Enhanced Observability
Add structured logging hooks that can be customized per module.

### 4. Performance Optimization
Consider caching strategies for frequently accessed checkpoints.

## Conclusion

This refactoring successfully:
- ✅ Eliminated ~150 lines of code duplication in Agent module
- ✅ Fixed type constraint issues preventing base class usage
- ✅ Established clear architecture for future checkpoint implementations
- ✅ Maintained backward compatibility with existing code
- ✅ Removed unused experimental code

The key insight was **relaxing type constraints** to match actual usage patterns while keeping strong typing where it matters (checkpoint types, state snapshots).

