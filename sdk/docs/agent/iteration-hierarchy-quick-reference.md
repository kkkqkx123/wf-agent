# Agent Loop Architecture - Quick Reference

## Key Concepts

### 1. Iteration Hierarchy

```
Workflow Execution (Level 0)
  └─ Agent Loop (Level 1)
       ├─ Iteration #1 (Level 2)
       │    └─ Sub-Agent (Level 3)
       ├─ Iteration #2 (Level 2)
       └─ Iteration #N (Level 2)
```

**Each iteration has:**
- Isolated variable scope
- Separate message history
- Independent tool call tracking
- Automatic cleanup after completion

### 2. Variable Resolution Priority

```
1. Current Iteration Scope (writeable)
   ↓ not found
2. Agent-Loop Scope (writeable)
   ↓ not found
3. Parent Workflow Scope (read-only)
   ↓ not found
4. Global Scope (read-only)
```

### 3. API Migration

| Old API | New API | Status |
|---------|---------|--------|
| `setVariable(name, value)` | `setScopedVariable(name, value)` | Deprecated |
| `getVariable(name)` | `getScopedVariable(name)` | Deprecated |
| N/A | `iterationContextManager.enterIteration(n)` | New |
| N/A | `iterationContextManager.setVariable(name, value)` | New |

---

## Usage Examples

### Basic Iteration Scoping

```typescript
// Enter iteration scope
entity.iterationContextManager.enterIteration(1);

// Set iteration-local variable
entity.setScopedVariable('temp_result', { data: '...' });

// Access in same iteration
const result = entity.getScopedVariable('temp_result');

// Exit iteration (auto-cleanup later)
entity.iterationContextManager.exitIteration();

// Next iteration has clean slate
entity.iterationContextManager.enterIteration(2);
const prevResult = entity.getScopedVariable('temp_result'); // undefined
```

### Sub-Agent Communication

```typescript
// Parent agent sets shared context
entity.setScopedVariable('task', 'analyze code');

// Sub-agent inherits read-only access
const task = subAgentEntity.getScopedVariable('task'); // 'analyze code'

// Sub-agent sets its own results
subAgentEntity.setScopedVariable('analysis', { ... });

// On completion, auto-sync to parent (with prefix)
// Parent can access: $subagent.{id}.analysis
```

### Hook Integration

```typescript
{
  hookType: 'AFTER_ITERATION',
  handler: async (context) => {
    // Access current iteration state
    const currentVars = context.entity.iterationContextManager.getAllVariables();
    
    // Compare with previous iteration
    const prevCtx = context.entity.iterationContextManager.getIterationContext(
      context.iteration - 1
    );
    
    if (prevCtx) {
      console.log('Changes:', diff(prevCtx.variables, currentVars));
    }
  }
}
```

---

## Implementation Checklist

### Phase 1: Foundation ☐
- [ ] Create `IterationContextManager` class
- [ ] Add to `AgentLoopEntity` constructor
- [ ] Implement enter/exit lifecycle
- [ ] Write unit tests

### Phase 2: Integration ☐
- [ ] Implement scoped variable resolution
- [ ] Update checkpoint serialization
- [ ] Add cleanup logic
- [ ] Emit iteration events

### Phase 3: Sub-Agent Support ☐
- [ ] Implement sync strategies
- [ ] Add automatic sync on completion
- [ ] Create hook handlers
- [ ] Integration tests

### Phase 4: Migration ☐
- [ ] Deprecate old APIs
- [ ] Update documentation
- [ ] Migration guide
- [ ] Release notes

---

## Common Pitfalls

### ❌ Don't Mix APIs During Transition

```typescript
// BAD: Mixing old and new APIs
entity.setVariable('x', 1);           // Old API
const y = entity.getScopedVariable('x'); // New API - may not find it!

// GOOD: Use consistent API
entity.setScopedVariable('x', 1);
const y = entity.getScopedVariable('x');
```

### ❌ Don't Forget to Exit Iterations

```typescript
// BAD: Forgetting to exit
entity.iterationContextManager.enterIteration(1);
// ... do work ...
// Forgot to exit! Next iteration will fail

// GOOD: Always use try-finally
try {
  entity.iterationContextManager.enterIteration(1);
  // ... do work ...
} finally {
  entity.iterationContextManager.exitIteration();
}
```

### ❌ Don't Rely on Iteration State After Cleanup

```typescript
// BAD: Accessing cleaned-up iteration
entity.iterationContextManager.enterIteration(1);
entity.setScopedVariable('data', value);
entity.iterationContextManager.exitIteration();

// ... 15 iterations later ...
const oldData = entity.getScopedVariable('data'); // May be undefined!

// GOOD: Persist important data to agent-loop scope
entity.variableStateManager.setVariableValue('persistent_data', value, 'workflowExecution');
```

---

## Performance Considerations

### Memory Management

- Default retention: 10 completed iterations
- Configurable via `maxRetainedIterations`
- Completed iterations are eligible for cleanup
- Checkpoint persists all iterations indefinitely

### Variable Lookup Cost

- Iteration scope: O(1) - Map lookup
- Agent-loop scope: O(1) - Map lookup  
- Parent workflow: O(depth) - Registry traversal
- Total worst case: O(depth) where depth ≤ 10

### Optimization Tips

```typescript
// Cache frequently accessed variables
const config = entity.getScopedVariable('config');
for (let i = 0; i < 100; i++) {
  // Use cached config instead of repeated lookups
  process(config);
}

// Minimize cross-scope access
// BAD: Accessing parent scope in tight loop
for (const item of items) {
  const globalConfig = entity.getScopedVariable('global_config'); // Slow!
}

// GOOD: Cache once
const globalConfig = entity.getScopedVariable('global_config');
for (const item of items) {
  process(item, globalConfig); // Fast!
}
```

---

## Troubleshooting

### Variable Not Found

**Problem:** `getScopedVariable('x')` returns `undefined`

**Checklist:**
1. Is variable set in current iteration? → Check `iterationContextManager.getCurrentContext()`
2. Is variable set in agent-loop scope? → Check `variableStateManager.getAllVariables()`
3. Is there a parent workflow? → Check `getParentContext()`
4. Was iteration cleaned up? → Check `getIterationHistory()`

**Debug Code:**
```typescript
console.log('Iteration vars:', entity.iterationContextManager.getAllVariables());
console.log('Agent vars:', entity.variableStateManager.getAllVariables());
console.log('Parent:', entity.getParentContext());
console.log('History:', entity.iterationContextManager.getIterationHistory().length);
```

### State Not Persisting Across Iterations

**Problem:** Variable set in iteration 1 is gone in iteration 2

**Cause:** Variables are iteration-scoped by default

**Solution:**
```typescript
// For persistent state, use agent-loop scope directly
entity.variableStateManager.setVariableValue('persistent', value, 'workflowExecution');

// Or use the fallback behavior (no active iteration)
// Don't call enterIteration() for global state
```

### Sub-Agent Results Not Visible

**Problem:** Sub-agent completes but parent can't see results

**Checklist:**
1. Is sync strategy set to 'merge' or 'replace'? (default: 'isolate')
2. Are you checking prefixed keys? (`$subagent.{id}.{name}`)
3. Did sub-agent complete successfully? (check status)

**Solution:**
```typescript
// Configure sync when creating sub-agent
const subAgentConfig = {
  ...config,
  syncStrategy: 'merge',  // Enable automatic sync
};

// Or manually access sub-agent entity
const subAgentEntity = registry.get(subAgentId);
const results = subAgentEntity.getAllVariables();
```

---

## Further Reading

- [Full Architecture Design](./iteration-hierarchy-variable-architecture.md)
- [Agent Loop State Machine](../state-machine/agent-loop-state-machine.md)
- [Checkpoint Configuration](./checkpoint-config-source-analysis.md)
- [Parent-Child Relationships](../agent-parent-child-relationship-types.md)

---

## FAQ

**Q: Can I disable iteration scoping?**  
A: Yes, simply don't call `enterIteration()`. Variables will use agent-loop scope (old behavior).

**Q: How do I access previous iteration's state?**  
A: Use `iterationContextManager.getIterationContext(iterationNumber)`.

**Q: What happens to iteration state on checkpoint restore?**  
A: All retained iterations are restored via `restoreFromSnapshot()`.

**Q: Can sub-agents have their own sub-agents?**  
A: Yes, hierarchy supports arbitrary depth (limited by MAX_DEPTH = 10).

**Q: Is this backward compatible?**  
A: Yes, old `setVariable`/`getVariable` APIs still work during transition period.

**Q: When will old APIs be removed?**  
A: Planned for v2.0 release. Migration guide will be provided.
