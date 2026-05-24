# Remaining Optional Tasks

This document lists the integration tasks that were identified in the analysis but **not implemented** in the current phase, along with rationale and implementation notes.

---

## P0 Tasks (Skipped)

### 1. `setPath` integration into `variable-handler.ts`

**Why skipped**: The `variable-handler.ts` code updates a `workflowExecution.variables` array of objects (each with `.name`, `.value`, etc.). `setPath` operates on nested object properties by dot-path, not on array items matched by name. The existing find-and-update logic is clearer and more type-safe for this data structure.

**If needed in the future**: A utility function could be added to `path-resolver.ts` that finds/replaces array items by a key-value match:
```typescript
export function setArrayItemByKey(
  array: Record<string, unknown>[],
  keyField: string,
  keyValue: string,
  valueField: string,
  newValue: unknown
): boolean { ... }
```

Then `variable-handler.ts` would become:
```typescript
setArrayItemByKey(workflowExecution.variables, "name", config.variableName, "value", typedResult);
```

---

## P1 Tasks (Skipped)

### 2. `DependencyManager` integration into `route-handler.ts`

**Why skipped**: Route conditions are evaluated once per route node visit. The context (variables) changes between visits — the workflow execution has moved forward. DependencyManager's cache-hit ratio would be near zero. The ExpressionCompiler caching already optimizes the parsing step.

**If needed in the future**:
- Create a `DependencyManager` instance on the `WorkflowExecutionEntity` (persists across handler calls)
- Pass it to `routeHandler` via execution context
- Register each route condition by key `route:<nodeId>:<targetNodeId>`
- Use `evaluateIfChanged()` to skip re-evaluation if variables haven't changed

### 3. `DependencyManager` integration into `loop-end-handler.ts`

**Why skipped**: Same reasoning as route-handler. The break condition could be re-evaluated each loop iteration, but the loop variables (`__loop_state`, iteration variable) change on every iteration.

**If needed in the future**: Same approach as route-handler — inject a per-execution `DependencyManager` and register the break condition by key `loopBreak:<loopId>`.

### 4. `pathExists` as condition pre-check

**Why skipped**: `pathExists` checks whether a dot-path exists in an object. For trigger conditions and route conditions, we don't know which paths to check without parsing the expression. The `DependencyManager` already captures dependency paths via `ExpressionCompiler.compile().dependencies`. If a dependency path doesn't exist, the evaluator will return `undefined`, which evaluates as `false`. This is the correct behavior without an explicit pre-check.

**If needed in the future**: A pre-check could be added in `conditionEvaluator.evaluate()`:
```typescript
const compiled = expressionCompiler.compile(condition.expression);
const deps = compiled.dependencies;
for (const dep of deps) {
  if (!pathExists(dep, context.variables)) {
    logger.debug(`Dependency path '${dep}' does not exist in context`);
    return false;
  }
}
return Boolean(compiled.evaluate(context));
```
This would short-circuit evaluation when a required variable path is absent, avoiding partial evaluation errors. However, this duplicates the evaluator's own undefined-handling and adds overhead.

---

## P2 Tasks (Skipped)

### 5. `visualizeAST` / `traceEvaluation` integration beyond debug mode

**Current state**: `traceEvaluation` is already used by `ConditionEvaluator.evaluateWithTrace()` when debug mode is on.

**Future integration ideas**:
- `visualizeAST()` can be exposed via a `workflow.debugAst(expression)` API on the workflow entity
- Could be used in error messages when expression evaluation fails — instead of just the error message, include a visual AST dump
- Useful in dev/test tooling, not in production paths

### 6. `formatTrace` for structured debug output

**Current state**: Already used inside `ConditionEvaluator.evaluateWithTrace()`.

**Future**: The formatted trace could be emitted as structured metadata (JSON) instead of plain text, enabling UI-level debugging views.

---

## P3 Tasks (Skipped)

### 7. `ast-metadata` integration (`extractAllMetadata`, `findNodeAtPosition`, etc.)

**Why skipped**: These utilities are designed for IDE-like features (cursor position → AST node) and expression documentation (extract comments). No consumer currently needs them.

**Integration target**: A future workflow designer / expression editor component.

---

## Never-Integrate Modules (Confirmed)

| Module | Reason |
|--------|--------|
| `validatePath`, `validateArrayIndex`, `validateValueType`, `SECURITY_CONFIG` | Already internal — called by `expressionEvaluator` and `validateExpression()` |
| `validateComparisonTypes`, `validateArrayMethodResult`, `isValidNumber`, `isArray` | Internal type helpers for `expressionEvaluator` |
| `dslValidate`, `tokenizeExpression` | `dslValidate` is rolled into `validateExpression()`; `tokenizeExpression` is a low-level helper |

---

## Implementation Notes for Future Work

### Adding per-execution DependencyManager

A `DependencyManager` on `WorkflowExecutionEntity` would enable per-expression caching within a single execution:

```typescript
// WorkflowExecutionEntity
private depManager = new DependencyManager();

getDependencyManager(): DependencyManager {
  return this.depManager;
}
```

Then in route-handler:
```typescript
const depManager = executionEntity.getDependencyManager();
const key = `route:${node.id}:${route.targetNodeId}`;
const tracked = depManager.getTrackedExpression(key);
if (tracked) {
  result = depManager.evaluateIfChanged(key, context);
} else {
  depManager.register(key, route.condition.expression, context);
  result = depManager.getTrackedExpression(key)?.lastResult;
}
```

### Circular dependency resolution

`expression-evaluator.ts` imports `expressionCompiler` from `expression-compiler.ts`, and vice versa. This works because:
1. Neither module accesses the other's export at module-load time
2. Both are only used inside methods (`evaluate()` / `compile()`), which run after both modules are fully loaded

If a future refactor splits these modules further, extract the shared AST evaluation into a separate helper module to eliminate the cycle.
