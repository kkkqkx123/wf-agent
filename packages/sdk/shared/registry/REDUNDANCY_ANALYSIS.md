# Registry Module Redundancy Analysis

## Overview

Analysis of the `sdk/core/registry` directory to identify redundancy and unnecessary abstraction layers.

**Analysis Date**: 2026-06-15  
**Directory**: `sdk/core/registry`  
**Total Files**: 14 main registry files (excluding tests and utils)  
**Total Size**: ~200KB

---

## Current Structure

```
registry/
├── agent-profile-registry.ts    (3.8KB)
├── event-emitter.ts            (12KB)
├── event-registry.ts            (10.6KB)
├── execution-hierarchy-registry.ts (12.6KB)
├── fragment-registry.ts         (7.3KB)
├── hook-template-registry.ts    (13.9KB)
├── index.ts                     (1.8KB)
├── node-template-registry.ts    (15.4KB)
├── prompt-template-registry.ts  (5.6KB)
├── script-registry.ts           (21.3KB)
├── skill-registry.ts            (25KB)
├── ~~timeout-registry.ts~~      (removed, use executionEntity.timeoutManager directly)
├── tool-registry.ts             (20.7KB)
├── trigger-template-registry.ts (16.8KB)
├── types.ts                     (3.1KB)
└── utils/
    ├── agent-profile-storage-utils.ts (1.8KB)
    ├── generic-storage-utils.ts       (5.5KB)
    ├── hook-template-storage-utils.ts (2KB)
    ├── node-template-storage-utils.ts (2KB)
    ├── registry-utils.ts              (0.9KB)
    ├── script-storage-utils.ts       (1.7KB)
    ├── tool-storage-utils.ts         (1.6KB)
    ├── trigger-storage-utils.ts      (2KB)
    └── validation-utils.ts           (5.3KB)
```

---

## Identified Redundancy

### 1. Event System: Overlapping Responsibilities

**Files**:
- `event-emitter.ts` (12KB) - Single execution event emitter
- `event-registry.ts` (10.6KB) - Manages multiple emitter instances

**Problem**:
- `EventRegistry` mostly delegates to `ExecutionEventEmitter`
- Methods like `getEmitter()`, `on()`, `once()`, `waitFor()` are simple proxies
- Only `emit()` contains real logic (global listeners + metrics collection)

**Current Code Pattern**:
```typescript
// event-registry.ts
on<T extends BaseEvent>(eventType, listener, options) {
  const emitter = this.getEmitter(options.executionId);
  return emitter.on(eventType, listener, { filter: options.filter });
}
```

**Recommendation**: Merge into single `EventRegistry` class, maintain singleton pattern

**Expected Benefit**: Reduce 10.6KB, eliminate proxy layer

---

### 2. Execution Hierarchy Registry: Overly Complex

**File**: `execution-hierarchy-registry.ts` (12.6KB, 431 lines)

**Current Responsibilities**:
1. Basic CRUD (register/unregister/get/has)
2. Hierarchy traversal (getAllDescendants/getDirectChildren)
3. Batch cleanup (cleanupHierarchy)
4. Type filtering (getByType/getExecutionsByRoot)
5. Integrity validation (validateHierarchyIntegrity)
6. Orphaned reference cleanup (cleanupOrphanedReferences)
7. Root info repair (repairRootInfo)

**Problem**:
- Violates Single Responsibility Principle
- Validation/repair logic should be in separate service
- Core registry functionality mixed with business logic

**Recommendation**:
- Extract validation/repair logic to `HierarchyIntegrityService`
- Keep registry focused on registration/discovery (< 200 lines)
- Move hierarchy tree operations to dedicated service

**Expected Benefit**: Reduce 200+ lines, separate concerns

---

### 3. Timeout Registry: ✅ Resolved (Deleted)

**File**: `timeout-registry.ts` — **deleted**.

**Outcome**:
- The file was already a stub (deprecation notice only)
- `TimeoutManager` is now held directly on `ExecutionEntity` (`executionEntity.timeoutManager`)
- No actual code imported from `timeout-registry.ts`
- Related type definitions (`TimeoutRegistryConfig`, `ResolvedTimeoutRegistryConfig`, `DEFAULT_TIMEOUT_REGISTRY_CONFIG`) removed from `shared/types/timeout.ts`
- Outdated docs referencing TimeoutRegistry have been deleted

**Resolution**: File deleted, types cleaned up, docs removed.

---

### 4. Storage Utils: Repetitive Patterns

**Files**:
- `agent-profile-storage-utils.ts` (1.8KB)
- `hook-template-storage-utils.ts` (2KB)
- `node-template-storage-utils.ts` (2KB)
- `script-storage-utils-utils.ts` (1.7KB)
- `tool-storage-utils.ts` (1.6KB)
- `trigger-storage-utils.ts` (2KB)

**Problem**:
- Each registry implements similar persistence/load logic
- Duplicated patterns for:
  - Write-through caching
  - Storage adapter integration
  - Error handling

**Recommendation**:
- Extract generic storage adapter pattern
- Create base class or utility functions
- Reduce duplication across storage utils

**Expected Benefit**: Reduce code duplication, improve maintainability

---

### 5. Validation Utils: Low Utilization

**File**: `validation-utils.ts` (5.3KB)

**Problem**:
- Comprehensive validation utility functions
- Only used by some registries
- Many registries implement their own validation logic

**Recommendation**:
- Standardize validation approach across registries
- Either use validation-utils everywhere or remove it

**Expected Benefit**: Improve consistency, reduce duplication

---

## Refactoring Priority

| Priority | Item | Expected Benefit | Effort |
|----------|------|------------------|--------|
| **High** | Merge event-emitter + event-registry | Reduce 10.6KB, eliminate proxy layer | Medium |
| **High** | Simplify execution-hierarchy-registry | Reduce 200+ lines, separate concerns | Medium |
| **Medium** | Evaluate timeout-registry necessity | Potentially reduce 18.8KB | Low | ✅ Resolved — Deleted
| **Medium** | Extract storage utils common pattern | Reduce duplication | Medium |
| **Low** | Unify validation logic | Improve consistency | Low |

---

## Core Issues

### 1. Excessive Abstraction Layers

**Current Pattern**:
```
Registry → Manager → Utils → Storage Adapter
```

**Problem**:
- Too many layers of indirection
- Simple operations require multiple function calls
- Hard to trace execution flow

**Example**:
```typescript
// Current: Multiple layers for simple operation
const registry = new TimeoutRegistry();
const manager = registry.getManager(executionId);
const handle = manager.register(options);

// Simplified: Direct approach
const managers = new Map<string, TimeoutManager>();
const manager = managers.get(executionId) || createManager();
const handle = manager.register(options);
```

### 2. Unclear Responsibility Separation

**Issues**:
- Some registries contain business logic
- Some only forward calls to underlying managers
- Storage logic mixed with registry logic

**Principle**: Registry should focus on registration/discovery, not business logic

### 3. Storage Layer Duplication

**Current State**:
- Each registry implements its own storage utils
- Similar patterns repeated 6+ times
- Inconsistent error handling

**Solution**: Create unified storage adapter pattern

---

## Recommended Principles

### 1. Registry Responsibility

Registry should **ONLY** handle:
- Registration/unregistration
- Discovery (get/list/search)
- Lifecycle management (clear/cleanup)

Registry should **NOT** handle:
- Business logic (validation, transformation)
- Persistence (delegate to storage adapters)
- Complex queries (delegate to specialized services)

### 2. Abstraction Layer Guidelines

Before adding a new layer, ask:
1. Does this layer provide significant value?
2. Can the functionality be achieved without it?
3. Is the complexity justified by the benefit?

### 3. Storage Pattern

**Recommended Approach**:
```typescript
// Generic storage adapter interface
interface StorageAdapter<T> {
  save(key: string, value: T): Promise<void>;
  load(key: string): Promise<T | null>;
  delete(key: string): Promise<void>;
}

// Base registry with storage support
class PersistentRegistry<T> {
  constructor(
    private storage: StorageAdapter<T>,
    private validator: Validator<T>
  ) {}
  
  async register(key: string, value: T): Promise<void> {
    this.validator.validate(value);
    await this.storage.save(key, value);
    this.items.set(key, value);
  }
}
```

---

## Next Steps

### Phase 1: High Priority (Recommended)

1. **Merge event system**:
   - Combine `event-emitter.ts` and `event-registry.ts`
   - Maintain singleton pattern for per-execution isolation
   - Keep global listener and metrics functionality

2. **Simplify execution hierarchy**:
   - Extract validation/repair to `HierarchyIntegrityService`
   - Keep core registry < 200 lines
   - Move tree operations to service layer

### Phase 2: Medium Priority

3. **Evaluate timeout registry**:
   - Assess if registry layer is necessary
   - Consider direct Map-based approach
   - Keep only if tag indexing is critical

4. **Unify storage patterns**:
   - Create base storage adapter
   - Reduce duplication in storage utils
   - Standardize error handling

### Phase 3: Low Priority

5. **Standardize validation**:
   - Adopt validation-utils across all registries
   - Or remove and let registries validate independently

---

## Measurement

**Success Metrics**:
- Reduce total registry code by 30-40% (from ~200KB to ~120-140KB)
- Eliminate proxy layers
- Clear separation of concerns
- Consistent patterns across registries

**Validation**:
- All existing tests pass
- No breaking changes to public APIs
- Improved code readability and maintainability