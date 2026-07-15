# Plugin Architecture Design (Revised)

> Date: 2026-07-15
> Status: Draft
> Scope: `@wf-agent/sdk` + `@wf-agent/sdk-kit`

## 1. Background and Motivation

### 1.1 Current State Assessment

The SDK and SDK-Kit already have a plugin system implementation in `packages/sdk/plugin/` and `packages/sdk-kit/src/plugin/`. Analysis of the current design has identified several architectural issues:

| Issue | Severity | Description |
|-------|----------|-------------|
| Stub contribution types | High | 6/13 contribution types are NO-OP stubs with no implementation |
| Tight coupling to SDK internals | High | `ContributionManager` imports SDK internal types (`BaseFormatter`, `NodeHandlerFn`, `IToolExecutor`) |
| Parallel registry isolation | High | `ContributionManager` is a separate registry not synced to existing `ToolRegistry`, `NodeTemplateRegistry`, etc. |
| No real sandbox isolation | Medium | `PluginSandbox` only does timeout + error wrapping, no process/module isolation |
| Semver checking is a stub | Medium | `checkDependency()` always returns true |
| SDK-Kit uses `any` casts | Medium | `PluginManager` accesses SDK internals via `(this.sdk as any)` |
| No lifecycle events | Medium | No events emitted during plugin lifecycle |
| No hot-reload support | Low | Module cache not cleared on reload |
| No declarative permissions | Low | No capability declaration system |

### 1.2 Goals of the Revised Design

1. **Clean abstraction layer**: Plugin contributions use plugin-agnostic interfaces, not SDK internal types
2. **No stubs**: Every declared contribution type has a complete implementation
3. **Registry bridge**: Plugin contributions are automatically synced to existing SDK registries
4. **Type-safe SDK↔SDK-Kit contract**: Eliminate `any` casts via explicit typed properties
5. **Real sandbox isolation**: Use Node.js `vm` module for script-level isolation
6. **Complete semver checking**: Use the `semver` library for version compatibility
7. **Event-driven lifecycle**: Emit events for plugin lifecycle transitions
8. **Hot-reload support**: Module cache clearing for development
9. **Declarative permissions**: Plugin manifest declares required capabilities

---

## 2. Revised Architecture

### 2.1 Layered Design

```
┌──────────────────────────────────────────────────────────────────┐
│                         Application Layer                        │
│  ┌──────────────────────────────────────────────────────────────┐│
│  │                     SDK-Kit                                   ││
│  │  PluginManager (typed, no `any` casts)                       ││
│  │  PluginConfigurator                                          ││
│  └──────────────────────────┬───────────────────────────────────┘│
│                              │ typed contract                    │
│  ┌──────────────────────────▼───────────────────────────────────┐│
│  │                     SDK (core)                                ││
│  │  PluginEngine (orchestrator)                                  ││
│  │  ├─ PluginLoader         (discovery + loading)                ││
│  │  ├─ PluginRegistry       (active plugin records)              ││
│  │  ├─ PluginLifecycle      (state machine + hooks)              ││
│  │  ├─ ContributionManager  (typed contribution storage)         ││
│  │  ├─ DependencyResolver   (semver + topological sort)          ││
│  │  ├─ PluginSandbox        (vm-based isolation + timeout)       ││
│  │  └─ PluginEventBus       (lifecycle event emission)           ││
│  └──────────────────────────┬───────────────────────────────────┘│
│                              │ sync via Bridge                    │
│  ┌──────────────────────────▼───────────────────────────────────┐│
│  │                  Contribution Bridge Layer                    ││
│  │  Auto-syncs plugin contributions to existing SDK registries:  ││
│  │  ToolRegistry, NodeTemplateRegistry, EventRegistry,           ││
│  │  PromptTemplateRegistry, FragmentRegistry, SkillRegistry, ... ││
│  └──────────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 Design Principles

1. **Abstraction over Implementation**: Plugin interfaces define their own types, not SDK internals
2. **Complete Contracts**: If a contribution type is declared, it must be fully implemented
3. **Single Source of Truth**: Contributions are registered once, then bridged to existing registries
4. **Fail-Fast with Graceful Degradation**: Plugin failures are isolated, but errors are not silently swallowed
5. **Defense in Depth**: Sandbox isolation + permission checks + allowlist/blocklist

---

## 3. Plugin Abstraction Layer

### 3.1 Problem: Current Design Uses SDK Internal Types

```typescript
// Current (problematic): ContributionManager tightly coupled to SDK internals
import type { BaseFormatter } from "../../services/llm/formatters/base.js";
import type { IToolExecutor } from "../../services/tools/core/interfaces.js";
import type { NodeHandlerFn } from "../../workflow/execution/handlers/node-handlers/index.js";
```

This means any change to SDK internal types breaks the plugin API. Plugins must import SDK-internal modules, creating a fragile dependency.

### 3.2 Solution: Plugin-Agnostic Abstraction Interfaces

Define plugin-specific interfaces in the plugin module itself:

```typescript
// packages/sdk/plugin/contributions/abstractions.ts

/**
 * Plugin-level abstraction for a node handler.
 * Does NOT depend on SDK internal types.
 */
export interface PluginNodeHandler {
  /** Unique node type identifier (e.g. "CUSTOM_LLM") */
  readonly nodeType: string;
  /** Execute this node */
  execute(context: PluginExecutionContext, node: PluginNodeData): Promise<PluginNodeResult>;
  /** Optional template for DSL-based node configuration */
  template?: Record<string, unknown>;
}

/**
 * Plugin-level abstraction for a tool executor.
 * Does NOT depend on SDK internal IToolExecutor.
 */
export interface PluginToolExecutor {
  readonly toolType: string;
  createInstance(config: Record<string, unknown>): PluginToolInstance;
}

export interface PluginToolInstance {
  execute(params: Record<string, unknown>, context: PluginToolContext): Promise<PluginToolResult>;
}

/**
 * Plugin-level abstraction for an LLM provider.
 * Does NOT depend on SDK internal BaseFormatter.
 */
export interface PluginLLMFormatter {
  readonly provider: string;
  formatRequest(messages: PluginMessage[], config: PluginLLMConfig): PluginLLMRequest;
  parseResponse(raw: unknown): PluginLLMResponse;
  parseStreamChunk(chunk: unknown): PluginLLMStreamChunk | null;
}

/**
 * Plugin-level execution context (minimal, stable).
 */
export interface PluginExecutionContext {
  workflowId: string;
  executionId: string;
  variables: Record<string, unknown>;
  logger: PluginLogger;
}

export interface PluginNodeData {
  id: string;
  type: string;
  config: Record<string, unknown>;
  inputs: Record<string, unknown>;
}

export interface PluginNodeResult {
  outputs: Record<string, unknown>;
  next?: string;
  error?: string;
}

export interface PluginToolContext {
  executionId: string;
  logger: PluginLogger;
}

export interface PluginToolResult {
  success: boolean;
  data: unknown;
  error?: string;
}

export interface PluginMessage {
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  name?: string;
}

export interface PluginLLMConfig {
  model: string;
  temperature?: number;
  maxTokens?: number;
  [key: string]: unknown;
}

export interface PluginLLMRequest {
  url: string;
  headers: Record<string, string>;
  body: unknown;
}

export interface PluginLLMResponse {
  content: string;
  toolCalls?: Array<{ name: string; arguments: Record<string, unknown> }>;
  usage?: { promptTokens: number; completionTokens: number };
}

export interface PluginLLMStreamChunk {
  content?: string;
  toolCall?: { name: string; arguments: string };
  done: boolean;
}
```

### 3.3 Internal Bridge: Map Plugin Abstractions to SDK Internals

The bridge layer converts between plugin abstractions and SDK internal types:

```typescript
// packages/sdk/plugin/contributions/bridge.ts

import type { NodeHandlerFn } from "../../workflow/execution/handlers/node-handlers/index.js";
import type { PluginNodeHandler } from "./abstractions.js";

/**
 * Bridge: Converts a PluginNodeHandler to an SDK-internal NodeHandlerFn.
 * Only the bridge depends on SDK internal types, not the plugin interface.
 */
export function createSDKNodeHandler(pluginHandler: PluginNodeHandler): NodeHandlerFn {
  return async (globalContext, entity, node) => {
    const result = await pluginHandler.execute(
      {
        workflowId: entity.workflowId,
        executionId: entity.executionId,
        variables: entity.variables,
        logger: globalContext.logger,
      },
      {
        id: node.id,
        type: node.type,
        config: node.config,
        inputs: node.inputs,
      },
    );
    // Convert PluginNodeResult back to SDK internal format
    return convertToSDKNodeResult(result);
  };
}
```

### 3.4 Benefits

- **Plugin API is stable**: Changes to SDK internals only affect the bridge, not the plugin interface
- **Plugin authors import fewer SDK internals**: They only need `@wf-agent/sdk/plugin`
- **Testable**: Plugin abstractions can be tested without SDK internals
- **Versioned independently**: Plugin API version can be separate from SDK version

---

## 4. Contribution Type Completeness

### 4.1 Problem: 6/13 Contribution Types Are Stubs

The current `ContributionRegistrarImpl` has 6 methods that are NO-OP stubs:

| Method | Status | Has `ContributionManager` impl? | Has SDK registry? |
|--------|--------|--------------------------------|-------------------|
| `registerNodeType` | ✅ Implemented | ✅ | `NodeTemplateRegistry` |
| `registerToolType` | ✅ Implemented | ✅ | `ToolRegistry` |
| `registerLLMProvider` | ✅ Implemented | ✅ | `FormatterRegistry` |
| `registerFormatter` | ✅ Implemented | ✅ | `FormatterRegistry` |
| `registerEventHandler` | ✅ Implemented | ✅ | `EventRegistry` |
| `registerHookHandler` | ✅ Implemented | ✅ | `HookExecutor` |
| `registerMiddleware` | ✅ Implemented | ✅ | Execution pipeline |
| `registerEvaluator` | ❌ NO-OP | ❌ | ❌ No existing registry |
| `registerScriptExecutor` | ❌ NO-OP | ❌ | `ScriptRegistry` |
| `registerResource` | ❌ NO-OP | ❌ | `ResourceManager` |
| `registerPromptTemplate` | ❌ NO-OP | ❌ | `PromptTemplateRegistry` |
| `registerFragment` | ❌ NO-OP | ❌ | `FragmentRegistry` |
| `registerSkillLoader` | ❌ NO-OP | ❌ | `SkillRegistry` |

### 4.2 Principle: Interface Segregation

Instead of one monolithic `ContributionRegistrar` with 13 methods, use **multiple small interfaces**:

```typescript
// packages/sdk/plugin/contributions/registrar.ts

/** Node type contribution */
export interface NodeTypeRegistrar {
  registerNodeType(handler: PluginNodeHandler): void;
}

/** Tool type contribution */
export interface ToolTypeRegistrar {
  registerToolType(executor: PluginToolExecutor): void;
}

/** LLM provider contribution */
export interface LLMProviderRegistrar {
  registerLLMProvider(formatter: PluginLLMFormatter): void;
}

/** Event handler contribution */
export interface EventHandlerRegistrar {
  registerEventHandler(eventType: string, handler: PluginEventHandler): void;
}

/** Script executor contribution */
export interface ScriptExecutorRegistrar {
  registerScriptExecutor(language: string, executor: PluginScriptExecutor): void;
}

/** Resource contribution */
export interface ResourceRegistrar {
  registerResource(resource: PluginResource): void;
}

/** Prompt template contribution */
export interface PromptTemplateRegistrar {
  registerPromptTemplate(key: string, template: PluginPromptTemplate): void;
}

/** Fragment contribution */
export interface FragmentRegistrar {
  registerFragment(fragment: PluginFragment): void;
}

/** Skill loader contribution */
export interface SkillLoaderRegistrar {
  registerSkillLoader(loader: PluginSkillLoader): void;
}

/** Middleware contribution */
export interface MiddlewareRegistrar {
  registerMiddleware(middleware: PluginExecutionMiddleware): void;
}

/**
 * Combined registrar that plugins use.
 * Each sub-registrar is optional — plugins only implement what they need.
 */
export interface ContributionRegistrar {
  nodeTypes?: NodeTypeRegistrar;
  toolTypes?: ToolTypeRegistrar;
  llmProviders?: LLMProviderRegistrar;
  formatters?: FormatterRegistrar;
  eventHandlers?: EventHandlerRegistrar;
  hookHandlers?: HookHandlerRegistrar;
  scriptExecutors?: ScriptExecutorRegistrar;
  resources?: ResourceRegistrar;
  promptTemplates?: PromptTemplateRegistrar;
  fragments?: FragmentRegistrar;
  skillLoaders?: SkillLoaderRegistrar;
  middleware?: MiddlewareRegistrar;
}
```

### 4.3 Plugin Registration Pattern

```typescript
// Plugin implementation
export default {
  manifest: { id: '@scope/my-plugin', /* ... */ },

  registerContributions(registrar: ContributionRegistrar) {
    // Only implement what you need — no empty stubs
    registrar.nodeTypes?.registerNodeType({
      nodeType: 'CUSTOM_LLM',
      async execute(context, node) {
        // implementation
      },
    });

    registrar.toolTypes?.registerToolType({
      toolType: 'GRAPHQL',
      createInstance(config) {
        return new GraphQLToolInstance(config);
      },
    });
  },
};
```

### 4.4 Principle: Only Declare What's Implemented

Remove unimplemented contribution types from `ContributionType` until they are implemented:

**Phase 1 (immediate)**: Keep only the 7 implemented types:
- `node-type`, `tool-type`, `llm-provider`, `formatter`, `hook-handler`, `event-handler`, `middleware`

**Phase 2 (next)**: Add `script-executor`, `resource`, `prompt-template`, `fragment`, `skill-loader` when the `ContributionManager` and SDK registries are fully wired

**Phase 3 (future)**: Add `evaluator` when the SDK has an evaluator framework

---

## 5. Registry Bridge Pattern

### 5.1 Problem: ContributionManager Is Parallel to Existing Registries

The current `ContributionManager` stores contributions in its own internal Maps, but the SDK execution engine queries existing registries like `ToolRegistry`, `NodeTemplateRegistry`, `EventRegistry`, `ScriptRegistry`, `PromptTemplateRegistry`, `FragmentRegistry`, and `SkillRegistry`. Plugin contributions registered in `ContributionManager` are invisible to these registries.

### 5.2 Solution: ContributionBridge

```typescript
// packages/sdk/plugin/contributions/bridge.ts

/**
 * ContributionBridge syncs plugin contributions from ContributionManager
 * to the existing SDK registries.
 *
 * This ensures the existing execution engine can use plugin-contributed
 * types without modification.
 */
export class ContributionBridge {
  constructor(
    private contributionManager: ContributionManager,
    private registries: SDKRegistries,
  ) {}

  /**
   * Sync all contributions from a plugin to the SDK registries.
   */
  async syncPluginContributions(pluginId: string): Promise<void> {
    const contributions = this.contributionManager.getPluginContributions(pluginId);

    // Sync node types to NodeTemplateRegistry
    for (const nodeType of contributions.nodeTypes) {
      const sdkHandler = createSDKNodeHandler(nodeType);
      this.registries.nodeTemplateRegistry.register({
        type: nodeType.nodeType,
        handler: sdkHandler,
        config: nodeType.template,
      });
    }

    // Sync tool types to ToolRegistry
    for (const toolType of contributions.toolTypes) {
      const sdkExecutor = createSDKToolExecutor(toolType);
      this.registries.toolRegistry.registerExecutor(toolType.toolType, sdkExecutor);
    }

    // Sync LLM providers to FormatterRegistry
    for (const provider of contributions.llmProviders) {
      const sdkFormatter = createSDKFormatter(provider);
      this.registries.formatterRegistry.register(provider.provider, sdkFormatter);
    }

    // Sync event handlers to EventRegistry
    for (const [eventType, handler] of contributions.eventHandlers) {
      this.registries.eventRegistry.on(eventType, createSDKEventHandler(handler));
    }

    // Sync hook handlers to HookExecutor
    for (const [hookType, handler] of contributions.hookHandlers) {
      this.registries.hookExecutor.register(hookType, createSDKHookHandler(handler));
    }

    // Sync middleware to execution pipeline
    for (const middleware of contributions.middleware) {
      this.registries.executionPipeline.addMiddleware(
        middleware.phase,
        createSDKMiddleware(middleware),
      );
    }
  }

  /**
   * Remove all contributions from a plugin from the SDK registries.
   */
  async unsyncPluginContributions(pluginId: string): Promise<void> {
    const contributions = this.contributionManager.getPluginContributions(pluginId);

    for (const nodeType of contributions.nodeTypes) {
      this.registries.nodeTemplateRegistry.unregister(nodeType.nodeType);
    }
    for (const toolType of contributions.toolTypes) {
      this.registries.toolRegistry.unregisterExecutor(toolType.toolType);
    }
    // ... etc
  }
}

/**
 * Aggregated SDK registries for the bridge.
 */
export interface SDKRegistries {
  nodeTemplateRegistry: {
    register(entry: { type: string; handler: unknown; config?: unknown }): void;
    unregister(type: string): void;
  };
  toolRegistry: {
    registerExecutor(type: string, executor: unknown): void;
    unregisterExecutor(type: string): void;
  };
  formatterRegistry: {
    register(provider: string, formatter: unknown): void;
    unregister(provider: string): void;
  };
  eventRegistry: {
    on(event: string, handler: unknown): void;
    off(event: string, handler: unknown): void;
  };
  hookExecutor: {
    register(hookType: string, handler: unknown): void;
    unregister(hookType: string): void;
  };
  executionPipeline: {
    addMiddleware(middleware: unknown): void;
    removeMiddleware(id: string): void;
  };
}
```

### 5.3 Architecture Flow

```
Plugin.registerContributions()
  → ContributionRegistrar
    → ContributionManager (stores in internal maps)
      → ContributionBridge (syncs to existing SDK registries)
        → ToolRegistry, NodeTemplateRegistry, EventRegistry, ...
```

When a plugin is deactivated:
```
PluginLifecycle.deactivate()
  → ContributionBridge.unsyncPluginContributions()
    → ToolRegistry.unregisterExecutor(), NodeTemplateRegistry.unregister(), ...
  → ContributionManager.unregisterAll()
```

---

## 6. Type-Safe SDK↔SDK-Kit Contract

### 6.1 Problem: PluginManager Uses `any` Casts

```typescript
// Current: fragile and untyped
private getPluginEngine(): any {
  const sdk = this.sdk as any;
  const engine = sdk.pluginEngine || sdk._pluginEngine;
  return engine;
}
```

### 6.2 Solution: Explicit PluginEngine Property on SDKInstance

```typescript
// packages/sdk/api/shared/core/sdk-instance.ts

export class SDKInstance {
  /**
   * Plugin engine instance.
   * Available only when plugins are enabled.
   */
  readonly pluginEngine?: PluginEngine;

  constructor(options: SDKOptions) {
    // ...
    if (options.plugins?.enabled) {
      this.pluginEngine = new PluginEngine(container, {
        plugins: options.plugins,
        sdkVersion: this.sdkVersion,
      });
    }
  }
}
```

### 6.3 SDK-Kit PluginManager Uses the Typed Property

```typescript
// packages/sdk-kit/src/plugin/plugin.manager.ts

export class PluginManager {
  constructor(private sdk: SDK) {
    // TypeScript now knows this is PluginEngine | undefined
    if (!sdk.pluginEngine) {
      throw new KitError(
        'Plugin system is not available. Enable plugins in SDK options.',
        KitErrorCode.PLUGIN_NOT_AVAILABLE,
      );
    }
  }

  private get engine(): PluginEngine {
    return this.sdk.pluginEngine!;
  }

  async load(source: string): Promise<PluginInfo> {
    const records = await this.engine.discover();
    const record = records.find(r =>
      r.manifest.id === source || r.manifest.name === source,
    );
    if (!record) {
      throw new KitError(
        `Plugin '${source}' not found`,
        KitErrorCode.RESOURCE_NOT_FOUND,
        { source },
      );
    }
    return this.toPluginInfo(record);
  }

  private toPluginInfo(record: PluginRecord): PluginInfo {
    return {
      id: record.manifest.id,
      version: record.manifest.version,
      name: record.manifest.name || record.manifest.id,
      status: record.status,
      contributions: Array.from(record.contributions, c => c.type),
      error: record.error?.message,
      activatedAt: record.activatedAt,
    };
  }
}
```

### 6.4 SDK Type Definition

```typescript
// packages/sdk-kit/src/types/sdk.types.ts

import type { PluginEngine } from "@wf-agent/sdk/plugin";

export interface SDK {
  /** Available when plugins are enabled */
  pluginEngine?: PluginEngine;

  // ... other SDK properties
  container: Container;
  workflows: WorkflowRegistry;
  executions: ExecutionRegistry;
  // ...
}
```

---

## 7. Real Sandbox Isolation

### 7.1 Problem: Current Sandbox Is Only Timeout + Error Wrap

The current `PluginSandbox`:
- Only wraps in `try/catch` and adds a timeout
- `enabled` defaults to `false`
- `maxMemory` is declared but never enforced
- `createContext()` returns a simple object that's never used
- No module-level or process-level isolation

### 7.2 Solution: Use Node.js `vm` Module for Script Isolation

```typescript
// packages/sdk/plugin/sandbox.ts

import vm from "vm";
import path from "path";

export class PluginSandbox {
  constructor(private options: PluginSandboxOptions) {}

  /**
   * Execute a plugin function in a restricted VM context.
   * Provides module-level isolation without requiring a separate process.
   */
  async execute<T>(pluginId: string, fn: () => Promise<T>): Promise<T> {
    if (!this.options.enabled) {
      return fn();
    }

    // 1. Timeout enforcement
    // 2. Error isolation
    // 3. Module access control
    // 4. Resource tracking
    return this.executeInSandbox(pluginId, fn);
  }

  private async executeInSandbox<T>(
    pluginId: string,
    fn: () => Promise<T>,
  ): Promise<T> {
    const context = this.createSandboxContext(pluginId);

    return new Promise<T>((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new PluginSandboxError(
          `Plugin '${pluginId}' timed out after ${this.options.timeout}ms`,
          pluginId,
        ));
      }, this.options.timeout);

      // Use vm.createContext for isolated global scope
      const vmContext = vm.createContext(context);

      vm.runInContext(
        `(async () => {
          const result = await fn();
          return result;
        })()`,
        vmContext,
        { timeout: this.options.timeout, filename: `plugin:${pluginId}` },
      )
        .then(result => {
          clearTimeout(timeout);
          resolve(result as T);
        })
        .catch(error => {
          clearTimeout(timeout);
          reject(new PluginSandboxError(
            `Plugin '${pluginId}' execution failed: ${error.message}`,
            pluginId,
            error,
          ));
        });
    });
  }

  private createSandboxContext(pluginId: string): Record<string, unknown> {
    const allowedModules: Record<string, unknown> = {};

    // Only provide access to whitelisted modules
    for (const moduleName of this.options.allowedModules) {
      try {
        allowedModules[moduleName] = require(moduleName);
      } catch {
        // Module not available, skip
      }
    }

    return {
      // Sandbox capabilities
      require: (moduleName: string) => {
        if (!this.options.allowedModules.includes(moduleName)) {
          throw new Error(
            `Plugin '${pluginId}' attempted to access disallowed module '${moduleName}'`,
          );
        }
        return allowedModules[moduleName];
      },
      console: {
        log: (...args: unknown[]) => this.logger.info(`[${pluginId}]`, ...args),
        warn: (...args: unknown[]) => this.logger.warn(`[${pluginId}]`, ...args),
        error: (...args: unknown[]) => this.logger.error(`[${pluginId}]`, ...args),
      },
      // Prevent access to process, global, etc.
      process: undefined,
      global: undefined,
      globalThis: undefined,
      __dirname: undefined,
      __filename: undefined,
    };
  }

  /**
   * Load a plugin module in a restricted context.
   * Instead of using `import()`, use `vm.Script` to compile and run.
   */
  async loadModuleInSandbox(
    pluginId: string,
    modulePath: string,
  ): Promise<Plugin> {
    if (!this.options.enabled) {
      return import(modulePath).then(m => m.default);
    }

    // Verify path is within allowed paths
    if (!this.isPathAllowed(modulePath)) {
      throw new PluginSandboxError(
        `Plugin '${pluginId}' module path '${modulePath}' is not allowed`,
        pluginId,
      );
    }

    const code = await fs.readFile(modulePath, 'utf-8');
    const script = new vm.Script(code, {
      filename: modulePath,
      timeout: this.options.timeout,
    });

    const sandboxContext = this.createSandboxContext(pluginId);
    const vmContext = vm.createContext(sandboxContext);

    try {
      script.runInContext(vmContext, {
        timeout: this.options.timeout,
        breakOnSigint: true,
      });

      // After execution, the module's exports should be in the context
      return vmContext.module?.exports?.default;
    } catch (error) {
      throw new PluginSandboxError(
        `Failed to load plugin '${pluginId}' in sandbox: ${error instanceof Error ? error.message : String(error)}`,
        pluginId,
        error instanceof Error ? error : undefined,
      );
    }
  }
}
```

### 7.3 Sandbox Security Levels

| Level | Isolation | Overhead | Use Case |
|-------|-----------|----------|----------|
| 0 (None) | None | None | Trusted plugins, development |
| 1 (Module) | `vm` module isolation | Low | Third-party plugins |
| 2 (Process) | `child_process.fork()` | Medium | Untrusted plugins |
| 3 (Container) | Docker container | High | Multi-tenant SaaS |

---

## 8. Plugin Lifecycle Events

### 8.1 Problem: No Events During Plugin Lifecycle

Other SDK components (UI, metrics, logging) cannot observe plugin lifecycle transitions.

### 8.2 Solution: PluginEventBus

```typescript
// packages/sdk/plugin/event-bus.ts

export type PluginEventType =
  | 'plugin:discovered'
  | 'plugin:loading'
  | 'plugin:loaded'
  | 'plugin:activating'
  | 'plugin:activated'
  | 'plugin:deactivating'
  | 'plugin:deactivated'
  | 'plugin:error'
  | 'plugin:config-changed';

export interface PluginEvent {
  type: PluginEventType;
  pluginId: string;
  timestamp: number;
  data?: Record<string, unknown>;
}

export class PluginEventBus {
  private listeners = new Map<PluginEventType, Set<(event: PluginEvent) => void>>();

  on(type: PluginEventType, listener: (event: PluginEvent) => void): () => void {
    if (!this.listeners.has(type)) {
      this.listeners.set(type, new Set());
    }
    this.listeners.get(type)!.add(listener);
    return () => this.listeners.get(type)?.delete(listener);
  }

  emit(type: PluginEventType, pluginId: string, data?: Record<string, unknown>): void {
    const event: PluginEvent = { type, pluginId, timestamp: Date.now(), data };
    const listeners = this.listeners.get(type);
    if (listeners) {
      for (const listener of listeners) {
        try {
          listener(event);
        } catch {
          // Don't let listener errors propagate
        }
      }
    }
  }

  removeAll(): void {
    this.listeners.clear();
  }
}
```

### 8.3 Integration with PluginLifecycleManager

```typescript
// In PluginLifecycleManager.activate():
this.eventBus.emit('plugin:activating', pluginId);
// ... activation logic ...
this.registry.updateStatus(pluginId, PluginStatus.ACTIVE);
this.eventBus.emit('plugin:activated', pluginId, { activatedAt: Date.now() });
```

---

## 9. Semver Checking

### 9.1 Problem: Placeholder Implementation

```typescript
// Current: always returns true
checkDependency(): boolean {
  const dep = _available.get(_depId);
  if (!dep) return false;
  return true; // TODO: semver
}
```

### 9.2 Solution: Use `semver` Library

```typescript
// packages/sdk/plugin/dependency-resolver.ts

import semver from "semver";

export class PluginDependencyResolver {
  checkDependency(
    pluginId: string,
    depId: string,
    depVersion: string,
    available: Map<string, PluginManifest>,
  ): boolean {
    const dep = available.get(depId);
    if (!dep) return false;

    // The depVersion is a semver range (e.g. "^1.0.0", ">=1.0.0 <2.0.0")
    // The dep.version is the actual version of the available dependency
    return semver.satisfies(dep.version, depVersion);
  }
}
```

And in `PluginLoader.validateManifest()`:

```typescript
validateManifest(manifest: PluginManifest, sdkVersion: string): ValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  // Validate required fields
  if (!manifest.id) errors.push("Missing 'id'");
  if (!manifest.version) errors.push("Missing 'version'");
  if (!manifest.entryPoint) errors.push("Missing 'entryPoint'");
  if (!manifest.sdkVersion) errors.push("Missing 'sdkVersion'");

  // Semver validation of SDK version compatibility
  if (manifest.sdkVersion && sdkVersion) {
    if (!semver.satisfies(sdkVersion, manifest.sdkVersion)) {
      errors.push(
        `SDK version '${sdkVersion}' does not satisfy plugin '${manifest.id}' requirement '${manifest.sdkVersion}'`,
      );
    }
  }

  // Validate manifest version is valid semver
  if (manifest.version && !semver.valid(manifest.version)) {
    errors.push(`Plugin '${manifest.id}' has invalid version '${manifest.version}'`);
  }

  return { valid: errors.length === 0, errors, warnings };
}
```

---

## 10. Hot-Reload Support

### 10.1 Problem: `import()` Cache Prevents True Reload

Node.js `import()` caches modules, so `reload()` (deactivate + activate) reuses the old module.

### 10.2 Solution: Module Cache Clearing

```typescript
// packages/sdk/plugin/loader.ts

export class PluginLoader {
  /**
   * Load a plugin module, with optional cache busting.
   */
  async loadModule(manifest: PluginManifest, bustCache = false): Promise<Plugin> {
    const entryPoint = manifest.entryPoint;
    const basePath = manifest._basePath || process.cwd();
    const modulePath = path.isAbsolute(entryPoint)
      ? entryPoint
      : path.resolve(basePath, entryPoint);

    if (bustCache) {
      // Clear the module from Node.js import cache
      this.clearModuleCache(modulePath);
    }

    // Use import() with a cache-busting query parameter
    const cacheBuster = bustCache ? `?v=${Date.now()}` : '';
    const moduleExports = await import(`${modulePath}${cacheBuster}`);
    return moduleExports.default as Plugin;
  }

  private clearModuleCache(modulePath: string): void {
    const resolved = path.resolve(modulePath);
    // Remove the module from the require cache
    delete require.cache[resolved];

    // Also clear all dependents that were loaded from the same directory
    for (const [key] of Object.entries(require.cache)) {
      if (key.startsWith(path.dirname(resolved))) {
        delete require.cache[key];
      }
    }
  }
}
```

### 10.3 PluginEngine Reload

```typescript
// packages/sdk/plugin/engine.ts

export class PluginEngine {
  /**
   * Reload a plugin: deactivate, clear cache, reload module, activate.
   * Supports hot-reload for development.
   */
  async reload(pluginId: string): Promise<void> {
    // 1. Deactivate existing plugin
    await this.deactivate(pluginId);

    // 2. Remove from registry
    this.registry.remove(pluginId);

    // 3. Discover and reload the plugin module
    const discovered = await this.loader.scanPlugins();
    const manifest = discovered.find(d => d.manifest.id === pluginId)?.manifest;
    if (!manifest) {
      throw new Error(`Plugin '${pluginId}' not found after re-scan`);
    }

    // 4. Load with cache busting
    const plugin = await this.loader.loadModule(manifest, true);

    // 5. Register and activate
    const record = this.registry.register(manifest, plugin);
    await this.activate(pluginId);
  }
}
```

---

## 11. Declarative Plugin Permissions

### 11.1 Problem: No Capability Declaration

Plugins can access any Node.js module or SDK service without declaring intent.

### 11.2 Solution: Add `capabilities` to PluginManifest

```typescript
// packages/sdk/plugin/types.ts

export interface PluginManifest {
  id: string;
  version: string;
  name?: string;
  description?: string;
  sdkVersion: string;
  entryPoint: string;
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  config?: Record<string, unknown>;
  contributions?: ContributionType[];
  hooks?: Partial<Record<PluginLifecycleHook, string>>;
  _basePath?: string;

  /**
   * Declared capabilities/permissions.
   * The sandbox uses this to determine what the plugin can access.
   */
  capabilities?: PluginCapability[];
}

export type PluginCapability =
  /** Network access (HTTP/HTTPS requests) */
  | 'network'
  /** File system read access */
  | 'fs-read'
  /** File system write access */
  | 'fs-write'
  /** Environment variable access */
  | 'env'
  /** Execute child processes */
  | 'process'
  /** Access SDK registries */
  | 'sdk:registries'
  /** Access SDK storage */
  | 'sdk:storage';
```

### 11.3 Permission Enforcement in Sandbox

```typescript
// In PluginSandbox
private enforceCapabilities(pluginId: string, manifest: PluginManifest): void {
  const capabilities = new Set(manifest.capabilities || []);

  // Build restricted module access based on declared capabilities
  const moduleAccess: Record<string, boolean> = {
    'fs': capabilities.has('fs-read') || capabilities.has('fs-write'),
    'fs/promises': capabilities.has('fs-read') || capabilities.has('fs-write'),
    'http': capabilities.has('network'),
    'https': capabilities.has('network'),
    'net': capabilities.has('network'),
    'child_process': capabilities.has('process'),
    'process': capabilities.has('env'),
    'os': capabilities.has('env'),
  };

  // Deny access to undeclared capabilities
  if (!capabilities.has('network')) {
    // Inject proxy that blocks network requests
    this.blockNetworkAccess(vmContext);
  }

  if (!capabilities.has('fs-read')) {
    this.blockFileSystemRead(vmContext);
  }
}
```

---

## 12. Complete File Structure (Revised)

### 12.1 SDK Plugin Module

```
packages/sdk/plugin/
├── index.ts                              # Public exports
├── types.ts                              # PluginManifest, Plugin, PluginContext, PluginCapability
├── engine.ts                             # PluginEngine (orchestrator)
├── loader.ts                             # PluginLoader (discovery + import + cache busting)
├── registry.ts                           # PluginRegistry (status tracking)
├── dependency-resolver.ts                # PluginDependencyResolver (semver + topological sort)
├── sandbox.ts                            # PluginSandbox (vm-based isolation + timeout)
├── lifecycle.ts                          # PluginLifecycleManager (state machine)
├── event-bus.ts                          # PluginEventBus (lifecycle events)
├── config.ts                             # OverridePolicy, mergePluginOptions
├── contributions/
│   ├── index.ts
│   ├── abstractions.ts                   # PluginNodeHandler, PluginToolExecutor, PluginLLMFormatter, etc.
│   ├── types.ts                          # ContributionType (only implemented types)
│   ├── manager.ts                        # ContributionManager
│   ├── registrar.ts                      # ContributionRegistrar (sub-interfaces + combined)
│   ├── bridge.ts                         # ContributionBridge (syncs to existing registries)
│   ├── middleware.types.ts               # MiddlewarePhase, ExecutionMiddleware
│   └── validation.ts                     # isValidContributionType, validateContribution
└── __tests__/
    ├── plugin-engine.test.ts
    ├── plugin-loader.test.ts
    ├── plugin-registry.test.ts
    ├── dependency-resolver.test.ts
    ├── sandbox.test.ts
    ├── event-bus.test.ts
    ├── contribution-manager.test.ts
    ├── bridge.test.ts
    └── integration/
        └── plugin-lifecycle.int.test.ts
```

### 12.2 SDK-Kit Plugin Module

```
packages/sdk-kit/src/plugin/
├── index.ts
├── plugin.manager.ts                     # PluginManager (typed, no `any` casts)
├── types.ts                              # PluginInfo
└── __tests__/
    └── plugin.manager.spec.ts
```

### 12.3 Modified Existing Files

```
packages/sdk/api/shared/core/sdk-instance.ts
  # Add `readonly pluginEngine?: PluginEngine` property
  # Remove `(this as any)._pluginEngine` hack

packages/sdk/api/shared/types/core-types.ts
  # Import PluginSystemOptions from plugin/types.ts (already done)

packages/sdk/di/container-config.ts
  # Simplify ContributionManager binding (no dynamic fallback)
  # Register PluginEngine in DI container

packages/sdk/di/service-identifiers.ts
  # PluginEngine, ContributionManager identifiers (already defined)

packages/sdk/index.ts
  # Export plugin module (already done)
```

---

## 13. Implementation Roadmap (Revised)

### Phase 1: Core Fixes (Foundation Repair)

| Step | Task | Package |
|------|------|---------|
| 1.1 | Define plugin abstraction interfaces (`PluginNodeHandler`, `PluginToolExecutor`, `PluginLLMFormatter` etc.) | sdk/plugin |
| 1.2 | Split `ContributionRegistrar` into sub-interfaces (interface segregation) | sdk/plugin |
| 1.3 | Remove stub contribution types from `ContributionType` and `ContributionRegistrar` | sdk/plugin |
| 1.4 | Implement `ContributionBridge` with sync to existing registries | sdk/plugin |
| 1.5 | Add `PluginEventBus` for lifecycle events | sdk/plugin |
| 1.6 | Implement proper semver checking | sdk/plugin |

### Phase 2: Type Safety

| Step | Task | Package |
|------|------|---------|
| 2.1 | Add typed `pluginEngine` property to `SDKInstance` | sdk/api |
| 2.2 | Update `PluginManager` to use typed property, remove `any` casts | sdk-kit |
| 2.3 | Simplify `ContributionManager` DI binding (remove dynamic fallback) | sdk/di |
| 2.4 | Add `PluginEngine` to `SDK` type in sdk-kit | sdk-kit/types |

### Phase 3: Sandbox Isolation

| Step | Task | Package |
|------|------|---------|
| 3.1 | Implement `vm`-based module isolation in `PluginSandbox` | sdk/plugin |
| 3.2 | Add module access control (whitelist) | sdk/plugin |
| 3.3 | Add `capabilities` to `PluginManifest` | sdk/plugin |
| 3.4 | Enforce capabilities in sandbox context | sdk/plugin |

### Phase 4: Hot-Reload + Advanced Features

| Step | Task | Package |
|------|------|---------|
| 4.1 | Add module cache clearing to `PluginLoader` | sdk/plugin |
| 4.2 | Add `reload()` to `PluginEngine` | sdk/plugin |
| 4.3 | Add `onConfigChange` event emission via `PluginEventBus` | sdk/plugin |
| 4.4 | Add plugin lifecycle integration tests | sdk/plugin |

### Phase 5: Remaining Contribution Types

| Step | Task | Package |
|------|------|---------|
| 5.1 | Implement `resource` contribution type + bridge to `ResourceManager` | sdk/plugin |
| 5.2 | Implement `script-executor` + bridge to `ScriptRegistry` | sdk/plugin |
| 5.3 | Implement `prompt-template` + bridge to `PromptTemplateRegistry` | sdk/plugin |
| 5.4 | Implement `fragment` + bridge to `FragmentRegistry` | sdk/plugin |
| 5.5 | Implement `skill-loader` + bridge to `SkillRegistry` | sdk/plugin |
| 5.6 | Add `evaluator` contribution type when SDK evaluator framework exists | sdk/plugin |

### Phase 6: Testing and Documentation

| Step | Task | Package |
|------|------|---------|
| 6.1 | Unit tests for all plugin core components | sdk/plugin |
| 6.2 | Integration tests for ContributionBridge | sdk/plugin |
| 6.3 | E2E tests for plugin lifecycle | sdk/__tests__ |
| 6.4 | Write plugin developer guide | docs/plugin |

---

## 14. Migration Guide

### 14.1 From Current to Revised

| Change | Impact | Migration |
|--------|--------|-----------|
| `ContributionRegistrar` → sub-interfaces | Breaking | Plugins need to use `registrar.nodeTypes?.registerNodeType(...)` instead of `registrar.registerNodeType(...)` |
| Remove stub types | Breaking | Remove calls to `registerEvaluator`, `registerScriptExecutor`, etc. |
| `PluginNodeHandler` replaces `NodeHandlerFn` | Breaking | Plugin authors need to wrap handlers in the new interface |
| `PluginLLMFormatter` replaces `BaseFormatter` | Breaking | Plugin LLM providers need to implement the new interface |
| Typed `pluginEngine` property | Additive | No migration needed for existing code |
| `PluginEventBus` | Additive | No migration needed |

### 14.2 Backward Compatibility Strategy

1. **Deprecation window**: Keep old `ContributionRegistrar` methods working for 2 minor versions, marked `@deprecated`
2. **Adapter layer**: Provide adapter functions that wrap old-style contributions into new abstractions
3. **Feature detection**: Plugins can check `if (registrar.nodeTypes)` to detect new API

---

## 15. Open Questions and Future Considerations

1. **Plugin registry**: Should there be a centralized plugin registry (like npm) for discovering and installing plugins?
2. **Plugin marketplace**: Future consideration for a plugin marketplace UI in the web-app
3. **Plugin bundling**: Should plugins be bundled (e.g., via Webpack/Rollup) for distribution?
4. **Plugin testing utilities**: Should `@wf-agent/sdk` provide test utilities for plugin developers?
5. **Plugin versioning**: Should the plugin API itself be versioned separately from the SDK?
6. **Performance impact**: What is the acceptable overhead of the `vm` sandbox and the middleware pipeline?
7. **Plugin scoping**: Should plugins be able to contribute new workflow node types that require custom validation logic?