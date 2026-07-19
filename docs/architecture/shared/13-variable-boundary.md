# Variable Boundary Data Passing

## 1. Overview

The variable boundary system manages explicit data passing between workflow execution scopes (parent-child, fork branches, loop iterations). It enforces a **no implicit scope inheritance** design — all cross-boundary variable transfers must be explicitly declared through typed mapping configurations.

### Design Philosophy

- **Explicit contracts**: Every variable crossing a boundary must be declared in a mapping configuration
- **Deep clone semantics**: All transferred variables are deep cloned to prevent accidental state pollution
- **Unified resolution**: A single path-expression model handles both flat names and nested access
- **Backward compatibility**: The legacy `externalName` field continues to work as syntactic sugar for simple paths

### Key Components

| Component | Responsibility |
|-----------|---------------|
| `VariableManager` | Runtime state management, variable storage, path resolution |
| `WorkflowVariableInput` | Declares how a parent variable maps to a child/internal variable |
| `WorkflowVariableOutput` | Declares how a child variable maps to a parent/external variable |
| `path-resolver.ts` | Low-level path expression parsing and resolution |
| Validators (Phase 2) | Static validation of mapping configurations at build time |

---

## 2. Core Types

### WorkflowVariableInput

Defines how external (parent) variables are mapped into the current workflow's internal scope:

```typescript
interface WorkflowVariableInput {
  /** External variable name (DEPRECATED: use sourcePath instead) */
  externalName?: string;

  /** Internal variable name (used within this workflow) */
  internalName: string;

  /**
   * Source path expression (takes precedence over externalName).
   * Supports nested access and array wildcards:
   * - "user.name" — nested property
   * - "items[0].title" — array index
   * - "docs[*].content" — array wildcard
   */
  sourcePath?: string;

  /** Whether this input is required */
  required?: boolean;

  /** Default value if the source variable is not found */
  defaultValue?: unknown;

  /** Description for documentation and tool hints */
  description?: string;
}
```

### WorkflowVariableOutput

Defines how internal variables are returned to the caller/parent workflow:

```typescript
interface WorkflowVariableOutput {
  /** Internal variable name (source within this workflow) */
  internalName: string;

  /** External variable name (target name in parent workflow) */
  externalName: string;

  /**
   * Target path expression (takes precedence over externalName).
   * Supports nested path writing:
   * - "output.result.answer" — write to nested path
   * - "data.items[0].score" — write to array index
   */
  targetPath?: string;

  /** Description for documentation */
  description?: string;
}
```

---

## 3. Path Expression Syntax

The system supports three levels of path expressions, unified under a single `resolvePath()` / `setPath()` API.

### Simple Path (Flat Name)

A plain variable name with no dots or brackets. Behaves identically to the legacy `externalName` lookup.

```
"userQuery"       → resolves to the variable named "userQuery"
"status"          → resolves to the variable named "status"
```

**Performance**: O(1) Map lookup via `getVariable()`, including cache and runtime validation.

### Nested Path (Dot Access)

Accesses properties within an object variable using dot notation.

```
"user.profile.name"          → resolves to user.profile.name
"config.output.result"       → resolves to config.output.result
"data.items[0].title"        → resolves to the title of the first item
```

**Performance**: O(n) object construction via `getAllVariables()` + path traversal. Result is cached for non-array values.

### Wildcard Path (Array Wildcard)

Collects values from all elements of an array using `[*]` syntax.

```
"docs[*].title"              → ["title1", "title2", ...] (array of titles)
"items[*].name"              → ["name1", "name2", ...]
"groups[*].items[*].id"      → flattened array of all IDs (multiple wildcards)
```

**Performance**: O(n) object construction + recursive traversal. Results are NOT cached (always returns fresh arrays).

---

## 4. Unified Resolution Engine

### `VariableManager.resolvePath(path)`

The unified read method that replaces the old dual-path branching:

```
resolvePath(path)
├── path is empty → return undefined
│
├── Simple path (no dots/brackets)
│   └── delegate to getVariable(path)  // O(1) Map lookup + cache + validation
│
└── Nested/wildcard path
    ├── data = getAllVariables()        // build flat Record
    ├── value = resolvePathWithWildcard(path, data)  // path traversal
    ├── Cache non-array result
    ├── Runtime validation (dev mode)
    └── return value
```

**Key behaviors**:
- Simple paths preserve the O(1) performance and cache behavior of `getVariable()`
- Nested paths only incur the `getAllVariables()` cost when actually needed
- Runtime validation extracts the root variable name from the path and validates it against the current node's declared inputs
- Cache TTL is configurable via `VariableManager` constructor options

### `VariableManager.setPath(path, value, options?)`

The unified write method that replaces the old `exportToFlatName()` / `exportToPath()` branching:

```
setPath(path, value)
├── Simple path (no dots)
│   ├── Variable exists → setVariable(path, value)
│   └── Variable missing → registerVariable(...)  // auto-create
│
└── Nested path (with dots)
    ├── rootName = parts[0]
    ├── rootObj = getVariable(rootName)
    │
    ├── rootObj is undefined
    │   ├── Create empty object
    │   └── registerVariable(rootName, object)
    │
    ├── rootObj is scalar
    │   ├── newObj = buildNestedObject(remainingParts, value)
    │   └── setVariable(rootName, newObj)
    │
    └── rootObj is object
        ├── clonedValue = structuredClone(value)
        ├── setPath(relativePath, rootObj, clonedValue)  // mutate in-place
        └── setVariable(rootName, rootObj)                // persist
```

---

## 5. Cross-Boundary Flow

### Import Flow (Parent → Child)

Used by SUBGRAPH, LOOP_START, and SYNC nodes to receive variables from a parent scope:

```
ParentVariableManager              ChildVariableManager
     │                                    │
     │  importVariables(source, inputs)   │
     │──────────────────────────────────► │
     │                                    │
     │  For each mapping:                 │
     │  ┌─────────────────────────────┐   │
     │  │ effectivePath = sourcePath  │   │
     │  │              ?? externalName│   │
     │  │ value = source.resolvePath  │   │
     │  │         (effectivePath)     │   │
     │  │                            │   │
     │  │ if value is undefined:     │   │
     │  │   ├── required → throw     │   │
     │  │   └── optional → use       │   │
     │  │       defaultValue or skip │   │
     │  │                            │   │
     │  │ if value is found:         │   │
     │  │   ├── structuredClone(value)│   │
     │  │   └── registerVariable(    │   │
     │  │       internalName, clone) │   │
     │  └─────────────────────────────┘   │
     │                                    │
```

**Isolation guarantee**: Imported values are deep cloned, so child mutations never affect parent state.

### Export Flow (Child → Parent)

Used by SUBGRAPH_END, LOOP_END, and JOIN nodes to return variables to a parent scope:

```
ChildVariableManager               ParentVariableManager
     │                                    │
     │  exportVariables(target, outputs)  │
     │──────────────────────────────────► │
     │                                    │
     │  For each mapping:                 │
     │  ┌─────────────────────────────┐   │
     │  │ value = this.getVariable    │   │
     │  │         (internalName)      │   │
     │  │                            │   │
     │  │ if value is undefined:     │   │
     │  │   └── skip (optional output)│   │
     │  │                            │   │
     │  │ if value is found:         │   │
     │  │   ├── structuredClone(value)│   │
     │  │   └── target.setPath(      │   │
     │  │       effectivePath, clone)│   │
     │  └─────────────────────────────┘   │
     │                                    │
```

**Important**: The `exportVariables` call is made on the **child** manager, targeting the **parent** manager. This follows the pattern "child exports to parent."

### Complete SUBGRAPH Cycle

```
Parent Workflow
│
├── SUBGRAPH_START
│   ├── variableInputs: [{ sourcePath: "config.user.name", internalName: "childName" }]
│   │
│   ├── createChildExecution(...)
│   │   └── child.importVariables(parent, inputs)
│   │       └── deep clone "config.user.name" → childName
│   │
│   ├── Subgraph Execution
│   │   └── child modifies childName independently
│   │
│   ├── Subgraph completes
│   │   └── parent.exportVariables(child, outputs)
│   │       └── deep clone childResult → parentOutput
│   │
│   └── SUBGRAPH_END
│
└── Parent continues with exported results
```

---

## 6. Validator Integration

### Builder-Level Validation (`workflow-validator.ts`)

Validates that `sourcePath` root variables exist in the parent workflow:

```typescript
// For SUBGRAPH and LOOP_START nodes:
const rootName = input.sourcePath
  ? input.sourcePath.split(".")[0]!.split("[")[0]!
  : input.externalName;
const parentVar = variables.find(v => v.name === rootName);
if (!parentVar && input.required && input.defaultValue === undefined) {
  errors.push(`Variable '${rootName}' not defined in parent`);
}
```

### Graph-Level Validation

| Validator | Check |
|-----------|-------|
| `subgraph-validator.ts` | Accepts `sourcePath` as alternative to `externalName`; error if neither is present |
| `sync-node-validator.ts` | Uses effective path (`sourcePath ?? externalName`) for deduplication |

### Zod Schema Validation (`boundary-config-schema.ts`)

Runtime validation of boundary configuration objects:

```typescript
WorkflowVariableInputSchema = z.object({
  externalName: z.string().optional(),
  internalName: z.string().min(1, "Internal name is required"),
  sourcePath: z.string().optional(),
  required: z.boolean().optional(),
  defaultValue: z.any().optional(),
  description: z.string().optional(),
}).refine(
  data => data.externalName || data.sourcePath,
  { message: "Either externalName or sourcePath is required" },
);
```

---

## 7. Backward Compatibility

### `externalName` as Syntactic Sugar

The `externalName` field remains in the API as syntactic sugar for `sourcePath: "name"` (the simple case). Internally, both are handled by the same `resolvePath()` / `setPath()` methods:

| Configuration | Effective Path | Behavior |
|---|---|---|
| `{ externalName: "x" }` | `"x"` | Simple O(1) Map lookup |
| `{ sourcePath: "x" }` | `"x"` | Same as above |
| `{ sourcePath: "a.b.c" }` | `"a.b.c"` | Nested path traversal |
| `{ sourcePath: "items[*].id" }` | `"items[*].id"` | Wildcard array expansion |
| `{ externalName: "x", sourcePath: "a.b" }` | `"a.b"` | `sourcePath` takes precedence |
| `{}` | `undefined` | Skipped with warning |

### Migration Path

Existing configurations using only `externalName` continue to work without modification:

```
// Before (legacy) — still works
{ externalName: "user_id", internalName: "uid", required: true }

// After (unified) — recommended for new code
{ sourcePath: "user.profile.id", internalName: "uid", required: true }

// Simple case — either works, both are equivalent
{ externalName: "name", internalName: "name" }
{ sourcePath: "name", internalName: "name" }
```

---

## 8. Implementation Reference

### Key Files

| File | Purpose |
|------|---------|
| `packages/types/src/workflow/boundary-config.ts` | Type definitions for `WorkflowVariableInput` / `WorkflowVariableOutput` |
| `packages/types/src/workflow/boundary-config-schema.ts` | Zod runtime validation schemas |
| `packages/sdk/workflow/execution/utils/variable-manager.ts` | `VariableManager` class with `resolvePath()`, `setPath()`, `importVariables()`, `exportVariables()` |
| `packages/sdk/services/evaluation/shared/path-resolver.ts` | Low-level `resolvePath()`, `resolvePathWithWildcard()`, `setPath()` functions |
| `packages/sdk/api/workflow/builders/workflow-validator.ts` | Builder-phase validation of variable mappings |
| `packages/sdk/workflow/validation/graph-validation/subgraph-validator.ts` | Graph-level SUBGRAPH validation |
| `packages/sdk/workflow/validation/graph-validation/sync-node-validator.ts` | Graph-level SYNC node validation |
| `packages/sdk/workflow/execution/handlers/node-handlers/sync-handler.ts` | SYNC node runtime handler using `resolvePath()` |

### Test Files

| File | Coverage |
|------|----------|
| `packages/sdk/workflow/state-managers/__tests__/variable-manager-subgraph.test.ts` | `importVariables`, `exportVariables`, `resolvePath`, `setPath`, sourcePath/targetPath integration |
| `packages/sdk/services/evaluation/__tests__/path-resolver.test.ts` | Low-level `resolvePath`, `resolvePathWithWildcard`, `setPath` |