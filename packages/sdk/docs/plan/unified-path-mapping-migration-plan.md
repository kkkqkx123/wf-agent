# Unified Path Mapping Migration Plan

## Problem Statement

The current implementation has two parallel code paths for variable mapping:

1. **Legacy path**: `externalName` → `source.getVariable(name)` (flat name lookup, with caching and runtime validation)
2. **New path**: `sourcePath` → `resolvePathWithWildcard(path, source.getAllVariables())` (path expression, without caching or validation)

This dual-path design creates:
- **Branching complexity** in `importVariables()` and `exportVariables()`
- **Inconsistent behavior**: the two paths have different caching, validation, and semantics
- **Migration inertia**: the new path cannot replace the old one until it matches all capabilities

## Goal

Enhance the `sourcePath`/`targetPath` mechanism to fully cover all capabilities of the legacy `externalName` path, then remove the legacy path entirely.

---

## Gap Analysis: What `externalName` Does That `sourcePath` Doesn't

### Gap 1: Caching

| Capability | `externalName` path | `sourcePath` path |
|---|---|---|
| Cache hit | `getVariable()` checks cache first | `getAllVariables()` always rebuilds the object |
| Cache write | `getVariable()` updates cache on miss | No cache |

**Impact**: Path-based imports bypass the `VariableManager`'s cache, causing repeated `getAllVariables()` calls for each mapping in the same batch.

### Gap 2: Runtime Validation

| Capability | `externalName` path | `sourcePath` path |
|---|---|---|
| `validateVariableAccess()` | Called in `getVariable()` development mode | Not called |
| Scope boundary check | Warns if accessing undeclared variable in subgraph/loop | No warning |

**Impact**: Path-based imports lose the development-mode guard that catches misuse of undeclared variables.

### Gap 3: Preprocessing Validation

| Location | Validates | Against |
|---|---|---|
| `workflow-validator.ts:137` | `input.externalName` | Parent workflow's `variables[]` definitions |
| `subgraph-validator.ts:125` | `input.externalName` non-empty | N/A |
| `sync-node-validator.ts:144` | `mapping.externalName` non-empty & uniqueness | N/A |

**Impact**: These validators cannot validate `sourcePath` because they don't know how to extract the root variable name from a path expression.

### Gap 4: Direct Variable Read (sync-handler)

**Location**: `sync-handler.ts:412`
```typescript
const value = sourceExecutionEntity.variableStateManager.getVariable(mapping.externalName);
```

**Impact**: The sync handler reads variables directly from the source branch's VariableManager to build the synced results record. With `sourcePath`, it would need to use the path resolver instead.

### Gap 5: Performance

| Metric | `externalName` path | `sourcePath` path |
|---|---|---|
| Time complexity | O(1) Map lookup | O(n) object construction + O(path) traversal |
| Memory | No allocation | New `Record` object per call |

**Impact**: For workflows with many variable mappings, repeated `getAllVariables()` calls create unnecessary GC pressure.

---

## Enhancement Design

### Enhancement 1: Add `resolvePathWithCache()` to VariableManager

Introduce a new method on `VariableManager` that combines path resolution with caching, serving as the unified replacement for both `getVariable()` and `resolvePathWithWildcard()`.

```typescript
class VariableManager {
  /**
   * Resolve a path expression from the variable store, with caching.
   *
   * - Simple paths (no dots, no brackets): O(1) Map lookup + cache
   * - Nested paths (with dots): O(n) getAllVariables() + path traversal + cache
   * - Wildcard paths (with [*]): O(n) getAllVariables() + recursive traversal
   *
   * Cache TTL is configurable via VariableManager constructor options.
   */
  resolvePath(path: string): unknown {
    // Check cache first (simple paths only, since nested paths depend on sub-objects)
    if (this.cacheEnabled && this.cache && !this.isNestedPath(path)) {
      const cached = this.cache.get(path);
      if (cached && Date.now() - cached.timestamp < this.cacheTTL) {
        return cached.value;
      }
    }

    // Simple path → O(1) Map lookup
    if (!this.isNestedPath(path)) {
      return this.getVariable(path); // includes cache + validation
    }

    // Nested/wildcard path → resolve from serialized data
    const data = this.getAllVariables();
    const value = resolvePathWithWildcard(path, data);

    // Cache simple result (single value, not array)
    if (this.cacheEnabled && this.cache && !Array.isArray(value)) {
      this.cache.set(path, { value, timestamp: Date.now() });
    }

    // Runtime validation for the root variable of the path
    if (process.env["NODE_ENV"] === "development" && this.executionEntity) {
      const rootName = path.split(".")[0]!.split("[")[0]!;
      this.validateVariableAccess(rootName);
    }

    return value;
  }

  private isNestedPath(path: string): boolean {
    return path.includes(".") || path.includes("[");
  }
}
```

**Key design decisions**:
- Simple paths (no dots/brackets) use the O(1) Map lookup, preserving performance
- Nested paths only incur `getAllVariables()` cost when actually needed
- Cache works for both simple and nested paths (cached by full path string)
- Runtime validation extracts the root name from the path and validates it

### Enhancement 2: Add `setPath()` to VariableManager

Add a unified write method that replaces both `setVariable(name, value)` and `exportToPath()`.

```typescript
class VariableManager {
  /**
   * Set a value at a path expression, with deep cloning.
   *
   * - Simple path (no dots): equivalent to setVariable(name, value)
   * - Nested path: traverse/create intermediate objects, set leaf value
   */
  setPath(path: string, value: unknown, options?: { freeze?: boolean }): void {
    const parts = path.split(".");
    const rootName = parts[0]!;

    if (parts.length === 1) {
      // Simple path: equivalent to setVariable
      this.setVariable(rootName, value, options?.freeze);
      return;
    }

    // Nested path: get or create root object, traverse, set
    let rootObj = this.getVariable(rootName);
    if (rootObj === undefined) {
      rootObj = {};
      this.registerVariable({
        name: rootName,
        type: "object",
        value: rootObj,
        readonly: false,
      });
    }

    if (typeof rootObj !== "object" || rootObj === null) {
      // Root is scalar, overwrite
      const newObj = buildNestedObject(parts.slice(1), value);
      this.setVariable(rootName, newObj, options?.freeze);
      return;
    }

    // Deep clone before mutation
    const clonedValue = structuredClone(value);
    const relativePath = parts.slice(1).join(".");
    setPath(relativePath, rootObj as Record<string, unknown>, clonedValue);
    this.setVariable(rootName, rootObj);
  }
}
```

### Enhancement 3: Update Preprocessing Validators

Validators need to understand `sourcePath` as a valid alternative to `externalName`.

#### 3.1 `workflow-validator.ts` — Validate that sourcePath root variable exists

```typescript
// Current: validates externalName against variable definitions
const parentVar = variables.find(v => v.name === input.externalName);

// Enhanced: validates either externalName or sourcePath root
const rootName = input.sourcePath
  ? input.sourcePath.split(".")[0]!.split("[")[0]!
  : input.externalName;
const parentVar = variables.find(v => v.name === rootName);
```

#### 3.2 `subgraph-validator.ts` — Accept sourcePath as valid alternative

```typescript
// Current: requires externalName to be non-empty
if (!input.externalName || !input.externalName.trim()) { /* error */ }

// Enhanced: accepts either sourcePath or externalName
if (!input.sourcePath && (!input.externalName || !input.externalName.trim())) {
  errors.push(`Missing sourcePath or externalName...`);
}
```

#### 3.3 `sync-node-validator.ts` — Accept sourcePath for uniqueness checks

```typescript
// Current: deduplicates by externalName
const key = mapping.externalName;

// Enhanced: deduplicates by effective path
const effectivePath = mapping.sourcePath ?? mapping.externalName;
```

### Enhancement 4: Update sync-handler for path-based reads

```typescript
// sync-handler.ts:412 — Current: direct getVariable call
const value = sourceExecutionEntity.variableStateManager.getVariable(mapping.externalName);

// Enhanced: use unified resolvePath
const value = sourceExecutionEntity.variableStateManager.resolvePath(
  mapping.sourcePath ?? mapping.externalName
);
```

### Enhancement 5: Make `externalName` Optional in Type Definition

```typescript
export interface WorkflowVariableInput {
  /** External variable name (used by caller/parent workflow) — DEPRECATED: use sourcePath instead */
  externalName?: string;

  /** Internal variable name (used within this workflow) */
  internalName: string;

  /**
   * Source path expression.
   * Supports nested access and array wildcards:
   * - "user.name" — nested property
   * - "items[0].title" — array index
   * - "docs[*].content" — array wildcard
   * - "userQuery" — simple name (equivalent to externalName: "userQuery")
   */
  sourcePath?: string;

  /** Whether this input is required */
  required?: boolean;

  /** Default value if external variable is not provided */
  defaultValue?: unknown;
}
```

**Note**: `externalName` becomes optional, not removed. This avoids breaking existing configurations while allowing new code to use only `sourcePath`.

---

## Migration Strategy

### Phase 1: Add `resolvePath()` and `setPath()` to VariableManager (current sprint)

| Step | File | Change |
|------|------|--------|
| 1.1 | `variable-manager.ts` | Add `resolvePath()` method (unifies `getVariable` + `resolvePathWithWildcard` + cache) |
| 1.2 | `variable-manager.ts` | Add `setPath()` method (unifies `setVariable` + `exportToPath`) |
| 1.3 | `variable-manager.ts` | Update `importVariables()` to use `resolvePath()` instead of branching |
| 1.4 | `variable-manager.ts` | Update `exportVariables()` to use `setPath()` instead of branching |
| 1.5 | `variable-manager.ts` | Remove `exportToFlatName()` and `exportToPath()` private methods |

**End state**: `importVariables` and `exportVariables` become single-path methods.

### Phase 2: Enhance Preprocessing Validators (next sprint)

| Step | File | Change |
|------|------|--------|
| 2.1 | `workflow-validator.ts` | Accept `sourcePath` as alternative to `externalName` for cross-reference validation |
| 2.2 | `subgraph-validator.ts` | Accept `sourcePath` as valid, require at least one of `sourcePath`/`externalName` |
| 2.3 | `sync-node-validator.ts` | Use effective path for deduplication |

### Phase 3: Update Consumer Sites (next sprint)

| Step | File | Change |
|------|------|--------|
| 3.1 | `sync-handler.ts` | Use `resolvePath()` instead of `getVariable(mapping.externalName)` |
| 3.2 | `predefined/workflow/*.ts` | (Optional) Migrate `externalName` to `sourcePath` for consistency |

### Phase 4: Type Cleanup (future)

| Step | File | Change |
|------|------|--------|
| 4.1 | `boundary-config.ts` | Make `externalName` optional in `WorkflowVariableInput` |
| 4.2 | `boundary-config-schema.ts` | Update Zod schema accordingly |
| 4.3 | All dependent files | Remove or update references assuming `externalName` is required |

---

## Backward Compatibility

### Phase 1-3

- All existing configurations using only `externalName` continue to work without modification
- `resolvePath("userQuery")` is semantically equivalent to `getVariable("userQuery")` for simple paths
- `setPath("status", value)` is semantically equivalent to `setVariable("status", value)` for simple paths

### Phase 4

- `externalName` becomes optional, but existing configs still parse correctly
- Migration path: `externalName: "x"` → `sourcePath: "x"` (or simply omit `sourcePath` and keep `externalName`)
- No forced migration: old configs work indefinitely

---

## Testing Strategy

### Unit Tests for `resolvePath()`

| Test Case | Input | Expected |
|-----------|-------|----------|
| Simple path, cache hit | `resolvePath("userQuery")` | Returns cached value, no Map lookup |
| Simple path, no cache | `resolvePath("userQuery")` | Returns `getVariable("userQuery")` result |
| Nested path | `resolvePath("user.profile.name")` | Returns nested value from `getAllVariables()` |
| Wildcard path | `resolvePath("docs[*].title")` | Returns array of titles |
| Nonexistent path | `resolvePath("missing.field")` | Returns `undefined` |

### Unit Tests for `setPath()`

| Test Case | Input | Expected |
|-----------|-------|----------|
| Simple path | `setPath("status", "done")` | Equivalent to `setVariable("status", "done")` |
| Nested path, root exists | `setPath("output.result", "ok")` | Mutates root object in-place |
| Nested path, root missing | `setPath("output.result", "ok")` | Creates root object and registers it |
| Nested path, root is scalar | `setPath("status.code", 200)` | Overwrites scalar root with object |

### Integration Tests

| Test Case | Scenario |
|-----------|----------|
| Subgraph with `sourcePath` | Subgraph imports nested data from parent via path expression |
| Subgraph with `externalName` | Existing subgraph continues to work (backward compat) |
| Fork with `targetPath` | Fork branch exports result to nested path in parent |
| Sync with `sourcePath` | Sync node resolves path expression across branches |
| Mixed mappings | Workflow uses both `externalName` and `sourcePath` in same mapping array |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `resolvePath()` performance regression for simple paths | Low | Medium | Simple paths bypass `getAllVariables()` and use O(1) Map lookup |
| Cache invalidation for nested paths | Medium | Low | Cache only simple results; wildcard/array results not cached |
| Validator false positives for `sourcePath` | Medium | Low | Validators only check root variable name, ignoring path depth |
| Migration of existing workflows | Low | Low | All existing configs continue to work; no forced migration |

---

## Summary

The enhancement strategy is **not about removing fields from the API, but about unifying the implementation under a single path-expression model**:

- `externalName` stays in the API as **syntactic sugar** for `sourcePath: "name"` (simple case)
- `sourcePath` is the **canonical form** that handles all cases (flat, nested, wildcard)
- Internally, `resolvePath()` and `setPath()` eliminate the dual-path branching
- Validators learn to accept both forms
- The result: one implementation path, two API shorthands `externalName`/`sourcePath` for user convenience

This gives us the best of both worlds: **clean internals** (no branching) + **friendly API** (concise `externalName` for simple cases).