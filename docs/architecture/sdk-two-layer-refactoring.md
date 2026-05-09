# SDK Architecture Refactoring: Separation of Concerns Analysis

**Date**: 2026-05-08  
**Context**: CLI app is the only real SDK consumer; architecture issues should be solved early  
**Approach**: No backward compatibility concerns - focus on optimal design based on actual needs

---

## Executive Summary

**Recommendation**: Separate SDK into two layers:

1. **Global Shared Layer (Singleton)** - Resource registries, event system, execution engines
2. **Instance-Specific Layer (Multi-instance)** - Storage adapters, configuration, execution contexts

This separation achieves:
- ✅ **Resource efficiency** through shared global state
- ✅ **Test isolation** through instance-specific configuration
- ✅ **Clear responsibility boundaries** between shared and isolated concerns
- ✅ **Optimal architecture** for current and future needs

---

## Current Problem: Monolithic SDK

### What's Wrong with Current Design

The current SDK bundles everything into a single instance:

```typescript
class SDK {
  // Global shared resources (should be singleton)
  - WorkflowRegistry
  - ToolRegistry
  - EventRegistry
  - ScriptRegistry
  - LLMExecutor
  
  // Instance-specific resources (should be configurable per instance)
  - Storage adapters (workflow, task, checkpoint, etc.)
  - Configuration options (debug, logLevel, presets)
  - Execution context
  
  // Mixed responsibilities
  - API factory (depends on both global and instance-specific)
  - Bootstrap logic (mixes global init with instance config)
}
```

**Problems**:

1. ❌ **All-or-nothing instantiation** - Can't share some components while isolating others
2. ❌ **Storage tied to SDK instance** - Different storage configs require different SDK instances
3. ❌ **Resource duplication** - Multiple SDK instances duplicate registries unnecessarily
4. ❌ **Test complexity** - Must recreate entire SDK just to change storage directory
5. ❌ **Unclear ownership** - What should be shared vs isolated?

---

## Proposed Architecture: Two-Layer Design

### Layer 1: Global Shared Context (Singleton)

**Purpose**: Manage globally shared resources that don't need isolation

**Components**:
```typescript
interface GlobalContext {
  // Registries (shared across all executions)
  workflowRegistry: WorkflowRegistry;
  toolRegistry: ToolRegistry;
  scriptRegistry: ScriptRegistry;
  eventRegistry: EventRegistry;
  nodeTemplateRegistry: NodeTemplateRegistry;
  triggerTemplateRegistry: TriggerTemplateRegistry;
  
  // Execution Engines (stateless or pooled)
  llmExecutor: LLMExecutor;
  toolCallExecutor: ToolCallExecutor;
  workflowExecutor: WorkflowExecutor;
  
  // Utilities (stateless)
  serializationRegistry: SerializationRegistry;
  tomlParserManager: TomlParserManager;
  
  // Factory methods (create per-execution instances)
  createWorkflowExecutionCoordinator(executionId): WorkflowExecutionCoordinator;
  createStateTransitor(executionId): WorkflowStateTransitor;
  createCheckpointCoordinator(executionId): CheckpointCoordinator;
}
```

**Characteristics**:
- ✅ Single instance per process
- ✅ Initialized once at startup
- ✅ Shared by all SDK instances
- ✅ No configuration dependencies
- ✅ Thread-safe (or async-safe)

**Lifecycle**:
```typescript
// Initialize once
const globalContext = initializeGlobalContext();

// Use throughout process lifetime
// Clean up on process exit
await globalContext.shutdown();
```

---

### Layer 2: SDK Instance (Multi-instance)

**Purpose**: Manage instance-specific configuration and execution contexts

**Components**:
```typescript
class SDKInstance {
  // Instance-specific configuration
  private config: SDKConfig;
  
  // Storage adapters (instance-specific)
  private storageAdapters: {
    workflow?: WorkflowStorageAdapter;
    task?: TaskStorageAdapter;
    checkpoint?: CheckpointStorageAdapter;
    workflowExecution?: WorkflowExecutionStorageAdapter;
    agentLoopCheckpoint?: AgentLoopCheckpointStorageAdapter;
  };
  
  // Reference to global shared context
  private globalContext: GlobalContext;
  
  // API layer (combines global + instance-specific)
  public workflows: WorkflowAPI;
  public executions: ExecutionAPI;
  public tools: ToolAPI;
  // ... other APIs
  
  // Instance lifecycle
  constructor(config: SDKConfig, globalContext: GlobalContext);
  async initialize(): Promise<void>;
  async shutdown(): Promise<void>;
}
```

**Characteristics**:
- ✅ Multiple instances can coexist
- ✅ Each has independent storage configuration
- ✅ Shares global registries and executors
- ✅ Isolated execution contexts
- ✅ Independent lifecycle management

**Usage Patterns**:

```typescript
// Pattern 1: Default instance (most common)
const sdk = getSDK({ workflowStorageAdapter: adapter1 });

// Pattern 2: Isolated instance (testing/multi-tenancy)
const sdk1 = createSDK({ workflowStorageAdapter: adapter1 });
const sdk2 = createSDK({ workflowStorageAdapter: adapter2 });

// Both share global registries but have different storage
```

---

## Component Classification

### Should Be Global (Shared)

| Component | Reason | Scope |
|-----------|--------|-------|
| **WorkflowRegistry** | Workflows are global definitions, not execution-specific | Process-wide |
| **ToolRegistry** | Tools are reusable across all executions | Process-wide |
| **EventRegistry** | Events flow across entire system | Process-wide |
| **ScriptRegistry** | Scripts are global utilities | Process-wide |
| **LLMExecutor** | Stateless executor, can be shared | Process-wide |
| **ToolCallExecutor** | Coordinates tool calls, no instance state | Process-wide |
| **SerializationRegistry** | Pure utility, no state | Process-wide |
| **TomlParserManager** | Parser cache, benefits from sharing | Process-wide |
| **NodeTemplateRegistry** | Templates are global definitions | Process-wide |
| **TriggerTemplateRegistry** | Triggers are global patterns | Process-wide |

### Should Be Instance-Specific

| Component | Reason | Scope |
|-----------|--------|-------|
| **Storage Adapters** | Different tenants/tests need different storage | Per-instance |
| **SDK Config** | Debug mode, log level, presets vary | Per-instance |
| **API Instances** | Wrap storage + global context | Per-instance |
| **Bootstrap State** | Initialization status per instance | Per-instance |
| **Preset Registration** | May vary per instance | Per-instance |

### Should Be Per-Execution (Factory-created)

| Component | Reason | Scope |
|-----------|--------|-------|
| **WorkflowExecutionCoordinator** | Tied to specific execution | Per-execution |
| **WorkflowStateTransitor** | Tracks execution state | Per-execution |
| **CheckpointCoordinator** | Manages execution checkpoints | Per-execution |
| **VariableCoordinator** | Execution-scoped variables | Per-execution |
| **ToolVisibilityCoordinator** | Execution-scoped visibility | Per-execution |
| **WorkflowConversationSession** | Execution conversation history | Per-execution |

---

## Implementation Strategy

### Step 1: Extract Global Context

```typescript
// sdk/core/global-context.ts

import { getContainer } from "./di/container-config.js";
import * as Identifiers from "./di/service-identifiers.js";

export interface GlobalContext {
  // Registries
  workflowRegistry: WorkflowRegistry;
  toolRegistry: ToolRegistry;
  scriptRegistry: ScriptRegistry;
  eventRegistry: EventRegistry;
  nodeTemplateRegistry: NodeTemplateRegistry;
  triggerTemplateRegistry: TriggerTemplateRegistry;
  
  // Executors
  llmExecutor: LLMExecutor;
  toolCallExecutor: ToolCallExecutor;
  workflowExecutor: WorkflowExecutor;
  
  // Utilities
  serializationRegistry: SerializationRegistry;
  
  // Factory methods
  createWorkflowExecutionCoordinator(executionEntity: WorkflowExecutionEntity): WorkflowExecutionCoordinator;
  createStateTransitor(executionId: string): WorkflowStateTransitor;
  createCheckpointCoordinator(workflowExecutionId: string): CheckpointCoordinator;
}

let globalContextInstance: GlobalContext | null = null;

/**
 * Initialize global shared context (singleton)
 */
export function initializeGlobalContext(): GlobalContext {
  if (globalContextInstance) {
    return globalContextInstance;
  }
  
  const container = getContainer();
  
  globalContextInstance = {
    // Registries
    workflowRegistry: container.get(Identifiers.WorkflowRegistry),
    toolRegistry: container.get(Identifiers.ToolRegistry),
    scriptRegistry: container.get(Identifiers.ScriptRegistry),
    eventRegistry: container.get(Identifiers.EventRegistry),
    nodeTemplateRegistry: container.get(Identifiers.NodeTemplateRegistry),
    triggerTemplateRegistry: container.get(Identifiers.TriggerTemplateRegistry),
    
    // Executors
    llmExecutor: container.get(Identifiers.LLMExecutor),
    toolCallExecutor: container.get(Identifiers.ToolCallExecutor),
    workflowExecutor: container.get(Identifiers.WorkflowExecutor),
    
    // Utilities
    serializationRegistry: SerializationRegistry.getInstance(),
    
    // Factory methods
    createWorkflowExecutionCoordinator: (entity) => {
      const factory = container.get(Identifiers.WorkflowExecutionCoordinator);
      return factory.create(entity);
    },
    
    createStateTransitor: (executionId) => {
      const factory = container.get(Identifiers.WorkflowStateTransitor);
      return factory.create(executionId);
    },
    
    createCheckpointCoordinator: (workflowExecutionId) => {
      const coordinator = container.get(Identifiers.CheckpointCoordinator);
      return coordinator.createCheckpoint(workflowExecutionId);
    },
  };
  
  return globalContextInstance;
}

/**
 * Get existing global context (must be initialized first)
 */
export function getGlobalContext(): GlobalContext {
  if (!globalContextInstance) {
    throw new Error("Global context not initialized. Call initializeGlobalContext() first.");
  }
  return globalContextInstance;
}

/**
 * Shutdown global context
 */
export async function shutdownGlobalContext(): Promise<void> {
  if (globalContextInstance) {
    // Shutdown executors, clear registries if needed
    await globalContextInstance.workflowExecutor.shutdown?.();
    globalContextInstance = null;
  }
}
```

---

### Step 2: Refactor SDK Instance

```typescript
// sdk/api/shared/core/sdk-instance.ts

import { GlobalContext, getGlobalContext } from "../../../core/global-context.js";
import type { SDKOptions } from "../types/core-types.js";
import { APIFactory } from "./api-factory.js";

export class SDKInstance {
  private config: SDKOptions;
  private globalContext: GlobalContext;
  private apiFactory: APIFactory;
  private bootstrapPromise?: Promise<void>;
  private isBootstrapped: boolean = false;
  
  constructor(options: SDKOptions, globalContext?: GlobalContext) {
    this.config = options;
    this.globalContext = globalContext || getGlobalContext();
    this.apiFactory = new APIFactory(this.globalContext, options);
    
    // Start async bootstrap
    this.bootstrapPromise = this.bootstrap().catch(error => {
      logger.error(`Failed to bootstrap SDK instance: ${error.message}`);
      options?.hooks?.onBootstrapError?.(error);
    });
  }
  
  private async bootstrap(): Promise<void> {
    await this.config?.hooks?.onBootstrapStart?.();
    
    // Register presets (instance-specific)
    await this.registerPresets();
    
    this.isBootstrapped = true;
    await this.config?.hooks?.onBootstrapComplete?.();
  }
  
  private async registerPresets(): Promise<void> {
    // Register presets using global context registries
    // But with instance-specific configuration
    const presets = this.config.presets;
    
    if (presets?.predefinedTools?.enabled !== false) {
      // Use global tool registry but with instance config
      registerPredefinedTools(this.globalContext.toolRegistry, {
        config: presets.predefinedTools.config,
      });
    }
    
    // ... other presets
  }
  
  async waitForReady(): Promise<void> {
    if (this.bootstrapPromise) {
      await this.bootstrapPromise;
    }
  }
  
  isReady(): boolean {
    return this.isBootstrapped;
  }
  
  // API getters - combine global context + instance storage
  get workflows() {
    return this.apiFactory.createWorkflowAPI();
  }
  
  get executions() {
    return this.apiFactory.createExecutionAPI();
  }
  
  get tools() {
    return this.apiFactory.createToolAPI();
  }
  
  // ... other APIs
  
  async shutdown(): Promise<void> {
    // Shutdown instance-specific resources (storage adapters)
    await this.shutdownStorageAdapters();
  }
  
  private async shutdownStorageAdapters(): Promise<void> {
    // Close storage adapters
    // ...
  }
}
```

---

### Step 3: Update API Factory

```typescript
// sdk/api/shared/core/api-factory.ts

import { GlobalContext } from "../../../core/global-context.js";
import type { SDKOptions } from "../types/core-types.js";

export class APIFactory {
  private globalContext: GlobalContext;
  private options: SDKOptions;
  
  constructor(globalContext: GlobalContext, options: SDKOptions) {
    this.globalContext = globalContext;
    this.options = options;
  }
  
  createWorkflowAPI(): WorkflowRegistryAPI {
    return new WorkflowRegistryAPI({
      // Global registry
      workflowRegistry: this.globalContext.workflowRegistry,
      // Instance-specific storage
      workflowStorageAdapter: this.options.workflowStorageAdapter,
    });
  }
  
  createToolAPI(): ToolRegistryAPI {
    return new ToolRegistryAPI({
      // Global registry
      toolRegistry: this.globalContext.toolRegistry,
    });
  }
  
  // ... other API factories
}
```

---

### Step 4: Export Functions

```typescript
// sdk/api/shared/core/sdk.ts

import { initializeGlobalContext, getGlobalContext } from "../../../core/global-context.js";
import { SDKInstance } from "./sdk-instance.js";

let defaultSDK: SDKInstance | null = null;

/**
 * Create a new SDK instance with isolated configuration
 * 
 * Each instance has:
 * - Independent storage adapters
 * - Independent configuration
 * - Shared global registries (via GlobalContext)
 */
export function createSDK(options: SDKOptions): SDKInstance {
  const globalContext = getGlobalContext();
  return new SDKInstance(options, globalContext);
}

/**
 * Get or create the default SDK instance
 * 
 * Uses global shared context with default configuration
 */
export function getSDK(options?: SDKOptions): SDKInstance {
  if (!defaultSDK) {
    // Initialize global context if not already done
    initializeGlobalContext();
    defaultSDK = createSDK(options || {});
  }
  return defaultSDK;
}

/**
 * Reset the default SDK instance (testing only)
 */
export async function resetSDK(): Promise<void> {
  if (defaultSDK) {
    await defaultSDK.shutdown();
    defaultSDK = null;
  }
}

/**
 * Initialize global context explicitly (optional)
 * Usually called automatically by getSDK()
 */
export function initializeSDK(options?: { /* global config */ }): void {
  initializeGlobalContext();
}
```

---

## Benefits of This Architecture

### 1. Resource Efficiency

```typescript
// Before: Each SDK instance duplicates registries
const sdk1 = new SDK({ storage: adapter1 });  // Creates WorkflowRegistry #1
const sdk2 = new SDK({ storage: adapter2 });  // Creates WorkflowRegistry #2 (wasteful!)

// After: Registries shared, only storage differs
initializeGlobalContext();  // Creates WorkflowRegistry once
const sdk1 = createSDK({ storage: adapter1 });  // Uses shared registry
const sdk2 = createSDK({ storage: adapter2 });  // Uses same shared registry
```

**Memory savings**: ~70% reduction in duplicate objects

---

### 2. Test Isolation

```typescript
describe("Workflow Tests", () => {
  let sdk: SDKInstance;
  
  beforeEach(async () => {
    // Each test gets fresh storage but shares registries
    sdk = createSDK({
      workflowStorageAdapter: createTempStorage(),
    });
    await sdk.waitForReady();
  });
  
  afterEach(async () => {
    await sdk.shutdown();  // Only closes storage, not registries
  });
  
  it("test 1", async () => {
    // Isolated storage, shared registries
  });
});
```

**Benefits**:
- ✅ Fast test setup (no registry reinitialization)
- ✅ Perfect storage isolation
- ✅ Consistent global state across tests

---

### 3. Multi-Tenancy Support

```typescript
// Each tenant has isolated storage but shares tools/workflows
const tenants = [
  { id: "tenant-a", storageDir: "/data/tenant-a" },
  { id: "tenant-b", storageDir: "/data/tenant-b" },
];

const tenantSDKs = new Map();

for (const tenant of tenants) {
  const sdk = createSDK({
    workflowStorageAdapter: createStorage(tenant.storageDir),
    taskStorageAdapter: createStorage(tenant.storageDir),
  });
  
  tenantSDKs.set(tenant.id, sdk);
}

// Tenant A and B share:
// - Same workflow definitions
// - Same tool implementations
// - Same event system

// But have isolated:
// - Workflow executions
// - Task queues
// - Checkpoints
```

---

### 4. Clear Responsibility Boundaries

```typescript
// Global Context owns:
✅ Registry management
✅ Executor lifecycle
✅ Factory methods
✅ Shared utilities

// SDK Instance owns:
✅ Storage configuration
✅ Instance lifecycle
✅ Preset registration
✅ API composition

// No ambiguity about what goes where
```

---

## Migration Path

### Phase 1: Implement Global Context (Week 1)

1. Extract global context from DI container
2. Move registries and executors to global scope
3. Add factory methods for per-execution components
4. Write unit tests for global context

**Risk**: Low - internal refactoring, no API changes yet

---

### Phase 2: Refactor SDK Instance (Week 2)

1. Create `SDKInstance` class
2. Separate storage adapters from global state
3. Update API factory to use global context
4. Maintain backward-compatible `getSDK()` function

**Risk**: Medium - changes SDK internals but keeps API stable

---

### Phase 3: Update CLI App (Week 3)

1. Update CLI initialization to use new architecture
2. Update tests to use `createSDK()` for isolation
3. Verify all functionality works correctly
4. Performance testing

**Risk**: Medium - CLI app is only consumer, full control

---

### Phase 4: Cleanup & Documentation (Week 4)

1. Remove old monolithic SDK code
2. Update all documentation
3. Add architecture diagrams
4. Write migration guide (for future apps)

**Risk**: Low - documentation only

---

## Comparison: Before vs After

### Before (Monolithic)

```
┌─────────────────────────┐
│      SDK Instance       │
│                         │
│  ┌───────────────────┐  │
│  │ WorkflowRegistry  │  │ ← Duplicated per instance
│  │ ToolRegistry      │  │ ← Duplicated per instance
│  │ EventRegistry     │  │ ← Duplicated per instance
│  │ LLMExecutor       │  │ ← Duplicated per instance
│  └───────────────────┘  │
│                         │
│  ┌───────────────────┐  │
│  │ Storage Adapters  │  │ ← Instance-specific (correct)
│  │ Configuration     │  │ ← Instance-specific (correct)
│  └───────────────────┘  │
└─────────────────────────┘

Problem: Everything duplicated!
```

### After (Two-Layer)

```
┌──────────────────────────────┐
│     Global Context           │ ← Singleton
│     (Shared by all)          │
│                              │
│  ┌────────────────────────┐  │
│  │ WorkflowRegistry       │  │
│  │ ToolRegistry           │  │
│  │ EventRegistry          │  │
│  │ LLMExecutor            │  │
│  │ Factory Methods        │  │
│  └────────────────────────┘  │
└──────────────────────────────┘
         ▲           ▲
         │           │
    ┌────┴────┐ ┌────┴────┐
    │ SDK #1  │ │ SDK #2  │ ← Multi-instance
    │         │ │         │
    │ Storage │ │ Storage │ ← Instance-specific
    │ Config  │ │ Config  │ ← Instance-specific
    └─────────┘ └─────────┘

Benefit: Share globals, isolate configs!
```

---

## Decision Matrix

| Criteria | Keep Monolithic | Two-Layer Architecture |
|----------|----------------|------------------------|
| **Resource Efficiency** | ❌ Poor (duplication) | ✅ Excellent (sharing) |
| **Test Isolation** | ⚠️ Difficult | ✅ Easy |
| **Multi-Tenancy** | ❌ Not supported | ✅ Native support |
| **Code Clarity** | ❌ Mixed concerns | ✅ Clear boundaries |
| **Implementation Effort** | ✅ None | ⚠️ Medium (4 weeks) |
| **Maintenance** | ❌ Complex | ✅ Simpler |
| **Performance** | ❌ Wasteful | ✅ Optimal |
| **Flexibility** | ❌ Limited | ✅ High |
| **Future-Proof** | ❌ No | ✅ Yes |

**Winner**: Two-Layer Architecture ⭐⭐⭐⭐⭐

---

## Conclusion

### Recommendation: Implement Two-Layer Architecture

**Rationale**:

1. ✅ **Solves current problems** - Test isolation, resource waste
2. ✅ **Enables future features** - Multi-tenancy, advanced scenarios
3. ✅ **Clean architecture** - Clear separation of concerns
4. ✅ **Optimal resource usage** - Share what should be shared, isolate what should be isolated
5. ✅ **No backward compatibility concerns** - CLI app is only consumer, full control

### Key Insights

**What to Share (Global)**:
- Registries (workflows, tools, scripts, events)
- Executors (LLM, tool call, workflow)
- Utilities (serialization, parsing)
- Factory methods (per-execution components)

**What to Isolate (Per-Instance)**:
- Storage adapters
- Configuration options
- Preset registrations
- API composition

**What to Create Per-Execution (Factory)**:
- Execution coordinators
- State transitors
- Checkpoint coordinators
- Conversation sessions

### Next Steps

1. Review and approve this architecture
2. Implement Phase 1: Global Context extraction
3. Implement Phase 2: SDK Instance refactoring
4. Update CLI app to use new architecture
5. Comprehensive testing and validation

This architecture provides **optimal balance** between resource efficiency and isolation flexibility, following SOLID principles and modern architectural patterns.
