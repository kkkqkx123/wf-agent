# Schema-Driven Mapping Improvement Plan

## Overview

This document proposes a phased improvement plan to evolve the current flattename-based variable mapping system toward a Schema-Driven Mapping architecture. The goal is to support path expressions (nested access, array wildcards) for data passing at all workflow boundaries (SUBGRAPH, FORK, JOIN, START, END, SYNC).

## Background

The current SDK implements data passing through `WorkflowVariableInput` / `WorkflowVariableOutput` with **flat name-to-name mapping**:

```typescript
// Current: flat rename only
{ externalName: "user_query", internalName: "query", required: true }
```

This is already declarative and better than hardcoded `state.get()`, but it cannot handle:
- Nested path access: `{ sourcePath: "$.data.user.name", internalName: "userName" }`
- Array wildcard: `{ sourcePath: "$.docs[*].content", internalName: "contexts" }`
- Nested output: `{ internalName: "answer", targetPath: "$.output.result.answer" }`

A detailed analysis is available in the [analysis report](../analysis/).

---

## Phase 1: Extend Boundary Config Types (P0)

### Problem

`WorkflowVariableInput` only supports `externalName` (flat source name) and `internalName` (flat target name). There is no way to express nested/transformed data access.

### Solution

#### 1.1 Add `sourcePath` to `WorkflowVariableInput`

**File**: `packages/types/src/workflow/boundary-config.ts`

Add an optional `sourcePath` field. When present, it takes precedence over `externalName` and supports path expressions.

**New interface**:

```typescript
export interface WorkflowVariableInput {
  /** External variable name (used by caller/parent workflow) - flat name lookup */
  externalName: string;

  /** Internal variable name (used within this workflow) */
  internalName: string;

  /**
   * Source path expression (optional, takes precedence over externalName).
   * Supports nested access and array wildcards:
   * - "user.name" - nested property
   * - "items[0].title" - array index
   * - "docs[*].content" - array wildcard (returns array of values)
   */
  sourcePath?: string;

  /** Whether this input is required */
  required?: boolean;

  /** Default value if external variable is not provided */
  defaultValue?: unknown;

  /** Description for documentation and tool hints */
  description?: string;
}
```

#### 1.2 Add `targetPath` to `WorkflowVariableOutput`

**File**: `packages/types/src/workflow/boundary-config.ts`

```typescript
export interface WorkflowVariableOutput {
  /** Internal variable name (source within this workflow) */
  internalName: string;

  /** External variable name (target for caller/parent workflow) - flat name */
  externalName: string;

  /**
   * Target path expression (optional, takes precedence over externalName).
   * Supports nested path writing:
   * - "output.result.answer" - write to nested path
   */
  targetPath?: string;

  /** Description for documentation */
  description?: string;
}
```

#### 1.3 Update Zod Schemas

**File**: `packages/types/src/workflow/boundary-config-schema.ts`

Add `sourcePath` and `targetPath` to the corresponding Zod schemas.

### Impact

- **Backward compatible**: existing configs without `sourcePath`/`targetPath` continue to work
- **No mandatory changes**: any consumer that doesn't need path expressions is unaffected

---

## Phase 2: Add Wildcard Path Resolution (P0)

### Problem

The existing `resolvePath()` in `services/evaluation/shared/path-resolver.ts` supports nested access and array indexing (`items[0].name`) but does NOT support array wildcards (`items[*].name`).

### Solution

#### 2.1 Add `resolvePathWithWildcard()`

**File**: `packages/sdk/services/evaluation/shared/path-resolver.ts`

Add a new function that extends `resolvePath()` with wildcard support:

```typescript
/**
 * Resolve path with wildcard array access support.
 * Extends resolvePath() to support [*] wildcard syntax.
 *
 * "items[*].name" → returns ["name1", "name2"] (array of name values)
 * "items[*]" → returns the array itself
 * "user.name" → same as resolvePath()
 */
export function resolvePathWithWildcard(path: string, root: unknown): unknown
```

**Behavior**:
- `[*]` at any path segment expands the remaining path across all array elements
- Multiple `[*]` segments are supported (cartesian product, though rare)
- Falls back to `resolvePath()` when no `[*]` is present

### Impact

- **New function**: does not modify existing `resolvePath()` behavior
- Used by `VariableManager.importVariables()` when `sourcePath` contains `[*]`

---

## Phase 3: Update VariableManager (P0)

### Problem

`VariableManager.importVariables()` and `exportVariables()` only do flat name lookups via `source.getVariable(name)` and `target.setVariable(name, value)`.

### Solution

#### 3.1 Update `importVariables()`

**File**: `packages/sdk/workflow/execution/utils/variable-manager.ts`

```typescript
importVariables(source: VariableManager, mappings: WorkflowVariableInput[]): void {
  for (const mapping of mappings) {
    let value: unknown;

    if (mapping.sourcePath) {
      // Path-based resolution (supports nested access and wildcards)
      const sourceData = source.getAllVariables();
      value = resolvePathWithWildcard(mapping.sourcePath, sourceData);
    } else {
      // Legacy flat name lookup (backward compatible)
      value = source.getVariable(mapping.externalName);
    }
    // ... rest of the logic unchanged (required/default/clone/register)
  }
}
```

#### 3.2 Update `exportVariables()`

**File**: `packages/sdk/workflow/execution/utils/variable-manager.ts`

```typescript
exportVariables(target: VariableManager, mappings: WorkflowVariableOutput[]): void {
  for (const mapping of mappings) {
    const value = this.getVariable(mapping.internalName);
    if (value !== undefined) {
      if (mapping.targetPath) {
        // Path-based writing: write to nested path in target's data
        const targetData = target.getAllVariables();
        setPath(mapping.targetPath, targetData, structuredClone(value));
        // Sync back individual mutated values
        syncNestedValues(target, mapping.targetPath, targetData);
      } else {
        // Legacy flat name write (backward compatible)
        // ... existing code ...
      }
    }
  }
}
```

**Note**: `setPath()` from `path-resolver.ts` mutates objects in-place, so we need to sync mutated values back to the target `VariableManager`. This requires a `syncNestedValues()` helper or a different approach.

**Better approach for export**: Instead of mutating the raw object, we can reconstruct the path. Since `VariableManager` stores variables flat, for nested paths we need to:
1. Get the root object that contains the path
2. Traverse to the target location
3. Set the value
4. Set the root back (if it changed)

Or simpler: use `setPath()` on the `getAllVariables()` result, then iterate and write back all changed top-level keys. However this is fragile. A cleaner approach:

**Alternative**: For now, only support `targetPath` that writes to a single flat key that is pre-registered in the target's VariableManager. This keeps things simple and works for the common case (e.g., `output.result` where `output` is a registered variable).

Actually, let's keep it even simpler: `targetPath` writes to the target's flat variable store by extracting the first segment of the path. If the target variable already exists (as a mutable object), we use `setPath` to mutate it in-place. This handles the common case of `output.result.answer` without needing to register nested variables.

### Impact

- **Backward compatible**: existing mappings without `sourcePath`/`targetPath` work identically
- Only the path expression logic is added

---

## Phase 4: Update Consumer Sites (P0)

### 4.1 Subgraph Handler

**File**: `packages/sdk/workflow/execution/handlers/node-handlers/subgraph-handler.ts`

No changes needed - the subgraph handler already delegates to `VariableManager.importVariables()` and `exportVariables()`, which are updated in Phase 3. The variable mapping config is passed through unchanged.

### 4.2 Fork Handler

**File**: `packages/sdk/workflow/execution/handlers/node-handlers/fork-handler.ts`

Same as Subgraph - no changes needed. Variable output mapping is handled by `VariableManager.exportVariables()` which is updated in Phase 3.

### 4.3 Execution Builder

**File**: `packages/sdk/workflow/execution/factories/workflow-execution-builder.ts`

The `initializeVariables()` method calls `importVariables()` - no changes needed as the method signature is unchanged.

### 4.4 End Handler

**File**: `packages/sdk/workflow/execution/handlers/node-handlers/end-handler.ts`

The end handler processes `dataOutputs` by reading variables and setting output keys. This is a different mechanism (data output, not variable export) and is out of scope for this phase.

---

## Migration Path

### Backward Compatibility

1. All existing `WorkflowVariableInput` configs without `sourcePath` continue to work
2. All existing `WorkflowVariableOutput` configs without `targetPath` continue to work
3. TypeScript types are extended, not changed

### Deprecation Strategy

- No deprecation needed for Phase 1-3
- Phase 4 (full schema-driven mapping) is future work and not covered here

### Testing

1. **Unit tests for `resolvePathWithWildcard()`**:
   - Nested access: `"user.name"` → correct value
   - Array index: `"items[0].title"` → correct value
   - Array wildcard: `"items[*].name"` → array of values
   - Multiple wildcards: `"groups[*].items[*].id"` → flattened array
   - No wildcard: falls back to `resolvePath()` behavior

2. **Unit tests for `importVariables()` with path**:
   - `sourcePath` present → uses path resolution
   - `sourcePath` absent → uses legacy `externalName` lookup
   - `sourcePath` with wildcard → imports array of values
   - Required path not found → throws `RuntimeValidationError`

3. **Unit tests for `exportVariables()` with path**:
   - `targetPath` present → writes to nested path
   - `targetPath` absent → uses legacy `externalName` write
   - Target path parent doesn't exist → creates intermediate objects

---

## Future Work (Not in Scope)

- **Node-level input/output schema contracts** (`getInputSchema()` / `getOutputSchema()`)
- **Preprocessing data flow analysis** (broken link detection, type mismatch)
- **Transform functions** (e.g., `join("\n", $.docs[*].content)`)
- **Dynamic mapping expressions** (runtime path resolution based on execution context)