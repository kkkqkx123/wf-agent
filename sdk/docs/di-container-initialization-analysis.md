# DI Container Initialization Issue Analysis

## Problem Summary

The SDK fails to initialize in tests with the error: **"No binding found for Symbol(GlobalContext)"**

This occurs because of a circular dependency and initialization order problem in the DI container setup.

## Root Cause Analysis

### 1. Initialization Order in SDKInstance Constructor

Current flow (lines 47-87 in `sdk-instance.ts`):

```typescript
constructor(options: SDKOptions) {
  // 1. Apply logging config
  this.applyLoggingConfig(options);
  
  // 2. Validate config
  this.validateConfig(options);
  
  // 3. Create container and configure ALL bindings
  const { container, containerId } = createIsolatedContainer({
    checkpoint: options?.checkpointStorageAdapter,
    // ... other adapters
  });
  
  // 4. Create GlobalContext (tries to resolve services from container)
  this.globalContext = new GlobalContext(container);  // ← PROBLEM HERE
  
  // 5. Bind GlobalContext to container (TOO LATE!)
  container.bind(ServiceIdentifiers.GlobalContext).toConstantValue(this.globalContext);
}
```

### 2. The Circular Dependency Chain

When `new GlobalContext(container)` is called at step 4, it tries to resolve multiple services:

```typescript
// In GlobalContext constructor (global-context.ts lines 64-74)
this.workflowRegistry = container.get(Identifiers.WorkflowRegistry);
this.toolRegistry = container.get(Identifiers.ToolRegistry);
this.scriptRegistry = container.get(Identifiers.ScriptRegistry);
this.eventRegistry = container.get(Identifiers.EventRegistry);
this.nodeTemplateRegistry = container.get(Identifiers.NodeTemplateRegistry);
this.triggerTemplateRegistry = container.get(Identifiers.TriggerTemplateRegistry);
this.llmExecutor = container.get(Identifiers.LLMExecutor);
this.toolCallExecutor = container.get(Identifiers.ToolCallExecutor);
this.workflowExecutor = container.get(Identifiers.WorkflowExecutor);  // ← TRIGGERS CHAIN
```

When resolving `WorkflowExecutor`, the container resolves its dependencies:

```
WorkflowExecutor
  └─> WorkflowExecutionCoordinator
      └─> TriggerCoordinator (Factory)
          └─> WorkflowExecutionBuilder
              └─> GlobalContext  ← CIRCULAR DEPENDENCY!
```

At line 586 in `container-config.ts`:
```typescript
const workflowExecutionBuilder = c.get(Identifiers.WorkflowExecutionBuilder);
```

And `WorkflowExecutionBuilder` binding (line 426):
```typescript
.toDynamicValue(c => {
  const globalContext = c.get(Identifiers.GlobalContext);  // ← FAILS HERE
  return new WorkflowExecutionBuilder(globalContext);
})
```

**The Problem**: `GlobalContext` hasn't been bound yet when this dynamicValue is evaluated!

### 3. Why DynamicValue Doesn't Help

The comment at line 76 says:
> "The bindings use lazy dynamicValue resolution, so they won't fail."

This is **incorrect**. While `toDynamicValue` is lazy, the issue is that `GlobalContext` constructor **eagerly resolves** services from the container, which triggers the resolution of services that depend on GlobalContext.

## Solutions

### Solution 1: Two-Phase Container Initialization (Recommended) ✅

**Approach**: Separate container creation into two phases:
1. Create container and bind GlobalContext
2. Configure remaining service bindings

**Implementation**:

```typescript
// In container-manager.ts
export function createIsolatedContainerTwoPhase(adapters: ContainerStorageConfig) {
  const containerId = generateContainerId();
  const container = new Container();
  
  // Phase 1: Bind storage adapters only
  if (adapters.checkpoint) {
    container.bind(Identifiers.CheckpointStorageAdapter)
      .toConstantValue(adapters.checkpoint);
  }
  // ... bind other adapters
  
  containerManager.getInstance().containers.set(containerId, container);
  
  return { container, containerId };
}

export function configureRemainingBindings(container: Container) {
  // Now configure all other service bindings
  // GlobalContext should already be bound at this point
  configureContainerBindings(container, {}); // adapters already bound
}
```

```typescript
// In sdk-instance.ts
constructor(options: SDKOptions) {
  this.config = options;
  this.applyLoggingConfig(options);
  this.validateConfig(options);
  
  // Phase 1: Create container with adapters only
  const { container, containerId } = createIsolatedContainerTwoPhase({
    checkpoint: options?.checkpointStorageAdapter,
    // ... other adapters
  });
  this.containerId = containerId;
  
  // Phase 2: Create and bind GlobalContext BEFORE other services
  this.globalContext = new GlobalContext(container);
  container.bind(ServiceIdentifiers.GlobalContext)
    .toConstantValue(this.globalContext);
  
  // Phase 3: Configure remaining service bindings
  configureRemainingBindings(container);
  
  // Now safe to create API factory
  this.apiFactory = new APIFactory(this.globalContext);
  
  this.bootstrapPromise = this.bootstrap().catch(error => {
    logger.error(`Failed to bootstrap SDK instance: ${getErrorMessage(error)}`);
    options?.hooks?.onBootstrapError?.(error);
  });
}
```

**Pros**:
- ✅ Clean separation of concerns
- ✅ No circular dependency
- ✅ Clear initialization order
- ✅ Easy to test

**Cons**:
- ⚠️ Requires refactoring `container-manager.ts`
- ⚠️ Breaking change to container creation API

---

### Solution 2: Lazy GlobalContext Resolution

**Approach**: Don't resolve services in GlobalContext constructor. Use lazy getters instead.

**Implementation**:

```typescript
// In global-context.ts
export class GlobalContext {
  private _workflowRegistry?: WorkflowRegistry;
  private _toolRegistry?: ToolRegistry;
  // ... other services
  
  constructor(readonly container: Container) {
    // Don't resolve anything here!
  }
  
  // Lazy getters
  get workflowRegistry(): WorkflowRegistry {
    if (!this._workflowRegistry) {
      this._workflowRegistry = this.container.get(Identifiers.WorkflowRegistry);
    }
    return this._workflowRegistry;
  }
  
  get toolRegistry(): ToolRegistry {
    if (!this._toolRegistry) {
      this._toolRegistry = this.container.get(Identifiers.ToolRegistry);
    }
    return this._toolRegistry;
  }
  
  // ... similar for all other services
}
```

**Pros**:
- ✅ No changes to container initialization
- ✅ Truly lazy loading
- ✅ Better performance (only load what's needed)

**Cons**:
- ⚠️ Significant refactoring of GlobalContext
- ⚠️ All code using GlobalContext must handle potential undefined values initially
- ⚠️ Loses type safety (can't guarantee services are available)

---

### Solution 3: Pre-bind GlobalContext Placeholder

**Approach**: Bind a placeholder/placeholder factory for GlobalContext before configuring other services.

**Implementation**:

```typescript
// In sdk-instance.ts
constructor(options: SDKOptions) {
  this.config = options;
  this.applyLoggingConfig(options);
  this.validateConfig(options);
  
  const { container, containerId } = createIsolatedContainer({
    checkpoint: options?.checkpointStorageAdapter,
    // ... other adapters
  });
  this.containerId = containerId;
  
  // Pre-bind a placeholder that will be replaced
  let globalContextRef: GlobalContext | null = null;
  container.bind(ServiceIdentifiers.GlobalContext)
    .toDynamicValue(() => {
      if (!globalContextRef) {
        throw new Error('GlobalContext not initialized yet');
      }
      return globalContextRef;
    })
    .inSingletonScope();
  
  // Now create GlobalContext (it can safely use the container)
  this.globalContext = new GlobalContext(container);
  globalContextRef = this.globalContext;  // Update the reference
  
  this.apiFactory = new APIFactory(this.globalContext);
  this.bootstrapPromise = this.bootstrap().catch(/*...*/);
}
```

**Pros**:
- ✅ Minimal changes to existing code
- ✅ Maintains current architecture

**Cons**:
- ⚠️ Hacky solution with closure trick
- ⚠️ Potential race conditions if services are resolved during GlobalContext construction
- ⚠️ Not thread-safe (though TS is single-threaded)

---

### Solution 4: Remove GlobalContext Dependencies from Constructor

**Approach**: Make GlobalContext accept services as constructor parameters instead of resolving them.

**Implementation**:

```typescript
// In global-context.ts
export class GlobalContext {
  constructor(
    readonly container: Container,
    readonly workflowRegistry: WorkflowRegistry,
    readonly toolRegistry: ToolRegistry,
    readonly scriptRegistry: ScriptRegistry,
    readonly eventRegistry: EventRegistry,
    readonly nodeTemplateRegistry: NodeTemplateRegistry,
    readonly triggerTemplateRegistry: TriggerTemplateRegistry,
    readonly llmExecutor: LLMExecutor,
    readonly toolCallExecutor: ToolCallExecutor,
    readonly workflowExecutor: WorkflowExecutor,
  ) {
    this.serializationRegistry = SerializationRegistry.getInstance();
  }
}
```

```typescript
// In sdk-instance.ts
constructor(options: SDKOptions) {
  this.config = options;
  this.applyLoggingConfig(options);
  this.validateConfig(options);
  
  const { container, containerId } = createIsolatedContainer({
    checkpoint: options?.checkpointStorageAdapter,
    // ... other adapters
  });
  this.containerId = containerId;
  
  // Manually resolve required services BEFORE creating GlobalContext
  const workflowRegistry = container.get(Identifiers.WorkflowRegistry);
  const toolRegistry = container.get(Identifiers.ToolRegistry);
  // ... resolve all other services
  
  // Now create GlobalContext with pre-resolved services
  this.globalContext = new GlobalContext(
    container,
    workflowRegistry,
    toolRegistry,
    // ... pass all services
  );
  
  container.bind(ServiceIdentifiers.GlobalContext)
    .toConstantValue(this.globalContext);
  
  this.apiFactory = new APIFactory(this.globalContext);
  this.bootstrapPromise = this.bootstrap().catch(/*...*/);
}
```

**Pros**:
- ✅ Explicit dependencies
- ✅ No circular dependency
- ✅ Easy to test (can mock services)

**Cons**:
- ⚠️ Still has the same problem - resolving services triggers the chain
- ⚠️ Verbose constructor
- ⚠️ Tight coupling to specific services

---

## Recommended Solution: Solution 1 (Two-Phase Initialization)

### Why This is Best:

1. **Clean Architecture**: Separates concerns clearly
2. **No Hacks**: Straightforward initialization order
3. **Testable**: Each phase can be tested independently
4. **Maintainable**: Easy to understand and modify
5. **Performance**: No lazy loading overhead

### Implementation Steps:

1. **Modify `container-manager.ts`**:
   - Add `createIsolatedContainerTwoPhase()` function
   - Add `configureRemainingBindings()` function
   - Keep existing `createIsolatedContainer()` for backward compatibility (deprecated)

2. **Modify `sdk-instance.ts`**:
   - Update constructor to use two-phase initialization
   - Ensure GlobalContext is bound before other services

3. **Update Tests**:
   - Tests can now create SDK instances successfully
   - Add tests for proper initialization order

4. **Documentation**:
   - Document the initialization order requirement
   - Explain why two-phase initialization is necessary

### Migration Path:

```typescript
// Old way (deprecated but still works)
const { container, containerId } = createIsolatedContainer(adapters);

// New way (recommended)
const { container, containerId } = createIsolatedContainerTwoPhase(adapters);
const globalContext = new GlobalContext(container);
container.bind(Identifiers.GlobalContext).toConstantValue(globalContext);
configureRemainingBindings(container);
```

## Additional Considerations

### Testing Strategy

Once fixed, tests should verify:
1. ✅ SDK can be created with minimal configuration
2. ✅ Multiple SDK instances can coexist
3. ✅ Services are properly isolated between instances
4. ✅ GlobalContext is available to all services
5. ✅ No circular dependency errors

### Performance Impact

- **Solution 1**: No performance impact (same number of operations, just reordered)
- **Solution 2**: Slightly better (lazy loading)
- **Solution 3**: Negligible impact
- **Solution 4**: No impact

### Backward Compatibility

- **Solution 1**: Breaking change to container API (but internal API)
- **Solution 2**: Breaking change to GlobalContext API
- **Solution 3**: No breaking changes
- **Solution 4**: Breaking change to GlobalContext constructor

Since this is an internal SDK API and the project is in development stage (per AGENTS.md: "No-backward-compatible"), breaking changes are acceptable.

## Conclusion

**Solution 1 (Two-Phase Initialization)** is the recommended approach because it:
- Fixes the root cause cleanly
- Improves code clarity
- Makes testing easier
- Has no performance penalty
- Is maintainable long-term

The key insight is that **GlobalContext must be bound to the container BEFORE any services that depend on it are configured**, but the current implementation tries to create GlobalContext AFTER all bindings are configured, creating a chicken-and-egg problem.
