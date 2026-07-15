# Plugin System Design Analysis

> Analysis date: 2026-07-15
> Scope: `packages/sdk/plugin/` (18 files, ~2700 lines) + `packages/sdk-kit/src/plugin/` (3 files)

---

## 1. Overall Architecture

The plugin system adopts a **two-layer architecture**:

| Layer | Package | Responsibility |
|---|---|---|
| **SDK Plugin Layer** | `packages/sdk/plugin/` | Core engine: discovery, loading, lifecycle, contribution management, dependency resolution, event bus |
| **SDK-Kit Management Layer** | `packages/sdk-kit/src/plugin/` | High-level API wrapper for application developers |

### Good design decisions

- **Separation of concerns**: Clear decomposition into Loader / Registry / Lifecycle / Guard / EventBus / DependencyResolver / ContributionManager / Bridge
- **Interface Segregation Principle**: 7 focused sub-interfaces (`NodeTypeRegistrar`, `ToolTypeRegistrar`, etc.) instead of one monolithic `ContributionRegistrar`
- **Plugin-agnostic abstractions**: `abstractions.ts` defines interfaces without depending on SDK internal types (`services/*`, `workflow/*`), allowing plugins to be developed independently
- **Clean lifecycle ordering**: `onLoad` → `registerContributions` → `bridge.sync` → `onActivate` (and reverse for deactivation)
- **Override policy**: Well-defined conflict resolution (FORBID / WARN / ALLOW / PRIORITY)
- **Guard pattern**: Timeout enforcement + error isolation for plugin execution stability

---

## 2. Features Provided

| Feature Area | Specific Capability | Implementation |
|---|---|---|
| **Plugin Discovery** | Scan from node_modules (`wfAgentPlugin` field), plugin.json directories, explicit paths | `loader.ts` |
| **Dynamic Loading** | `import()` based module loading with hot-reload cache busting | `loader.ts` |
| **Manifest Validation** | Required field checks, semver SDK compatibility, version format validation | `loader.ts` |
| **Dependency Resolution** | Topological sort (Kahn's algorithm), cycle detection, missing dependency detection, semver range checking | `dependency-resolver.ts` |
| **Lifecycle Management** | Strict 4-phase activation: `onLoad` → `registerContributions` → `bridge.sync` → `onActivate`; reverse deactivation | `lifecycle.ts` |
| **Status Tracking** | 8 states: DISCOVERED → LOADING → LOADED → ACTIVATING → ACTIVE → DEACTIVATING → DEACTIVATED → ERROR | `types.ts`, `registry.ts` |
| **Contribution Management** | Register/unregister/query for 7 contribution types (node-type, tool-type, llm-provider, formatter, hook-handler, event-handler, middleware) | `contributions/manager.ts` |
| **Override Policy** | 4 conflict strategies: FORBID / WARN / ALLOW / PRIORITY | `config.ts` |
| **Contribution Bridging** | Convert plugin abstractions to SDK internal types and sync to SDK registries | `contributions/bridge.ts` |
| **Guard Mechanism** | Plugin execution timeout control + error isolation (not a security sandbox) | `guard.ts` |
| **Event Bus** | Publish/subscribe for 9 lifecycle event types | `event-bus.ts` |
| **Access Control** | Plugin filtering via allowList / blockList | `lifecycle.ts` |
| **Runtime Config** | Plugin-scoped configuration injection and hot-reload callback | `lifecycle.ts`, `engine.ts` |
| **High-level API** | `PluginManager` with `load/activate/deactivate/list/get/reload/updateConfig` | `plugin.manager.ts` |

---

## 3. Issues Found

### 🔴 Critical

#### 1. `SDKRegistries` not passed to `PluginEngine` — bridge completely disabled

**File**: `packages/sdk/api/shared/core/sdk-instance.ts:569-575`

```typescript
const pluginEngine = new PluginEngine(
  this.globalContext.container,
  { plugins: this.config.plugins, sdkVersion: "1.0.0" },
  // ❌ No third argument — registries omitted, bridge is always undefined
);
```

The `PluginEngine` constructor creates `this.bridge = registries ? new ContributionBridge(...) : undefined`. Since `registries` is `undefined`, the bridge is **never created**.

**Impact**: When a plugin is activated, `lifecycle.ts`'s `activate()` method checks `if (this.bridge)` which is always `false`. `syncPluginContributions()` is never called. Plugin contributions (node types, tool types, LLM providers, etc.) exist only in `ContributionManager` memory but are **never synced to actual SDK registries** (`NodeTemplateRegistry`, `ToolRegistry`, `FormatterRegistry`, `EventRegistry`, `HookTemplateRegistry`). The execution engine cannot see any plugin contributions.

#### 2. Existing execution engine does not query `ContributionManager`

Even if `ContributionManager` contains plugin contributions, **no existing SDK component** (`NodeExecutionCoordinator`, `ToolRegistry`, `LLMExecutor`, etc.) reads from `ContributionManager` to resolve plugin-contributed handlers. The `ContributionManager` is an isolated registry with no consumer integration.

**Impact**: The contribution system is a write-only store — contributions can be registered but never consumed by the runtime.

---

### 🟡 Major

#### 3. Bridge unsync is incomplete (resource leaks on deactivation)

**File**: `packages/sdk/plugin/contributions/bridge.ts:143-217`

- **Tool cannot be unregistered**: `ToolRegistry` has `unregisterTool(toolId)`, but the bridge's `unsyncPluginContributions` explicitly skips `tool-type` entries (comment: "Tool entries are skipped here to avoid conflicts"). Deactivating a plugin leaves its tools registered.
- **Event handler cannot be unsubscribed**: `EventRegistry.onGlobal()` returns an unsubscribe function, but the bridge does not capture it. The unsync code skips `event-handler` entries (comment: "we didn't capture it during sync").

The code itself acknowledges these as known limitations via comments.

#### 4. `PluginManager.updateConfig()` does not update stored config

**File**: `packages/sdk-kit/src/plugin/plugin.manager.ts:152-176`

The method only calls `record.instance.onConfigChange(config)` but does **not update `PluginEngine.options.config[pluginId]`**. The engine continues to serve the old config. `updateConfig` is effectively a "notification" rather than an "update".

#### 5. Two `ContributionManager` instances possible

**Files**: `packages/sdk/di/container-config.ts:1366-1384`, `packages/sdk/plugin/engine.ts:66`

`container-config.ts` registers `ContributionManager` as a DI singleton, but `PluginEngine` creates its own internal instance. The DI version tries to detect if `PluginEngine` is initialized to reuse its instance, but the timing is fragile — if `PluginEngine` initialization fails, the DI creates a separate independent instance, splitting state.

#### 6. `PluginManager.load()` semantics are misleading

**File**: `packages/sdk-kit/src/plugin/plugin.manager.ts:27-62`

The method name `load(source)` suggests loading a single plugin by source, but it actually calls `discover()` (which scans ALL configured paths, returning cached results), then filters plugins by source. The method does `find` + optional `activate`, not "load" in the traditional sense.

---

### 🟢 Minor

#### 7. Dead code in `abstractions.ts`

**File**: `packages/sdk/plugin/contributions/abstractions.ts`

Defines `PluginScriptExecutor`, `PluginResource`, `PluginPromptTemplate`, `PluginFragment`, `PluginSkillLoader` interfaces, but `types.ts` explicitly states these are "removed until implemented" from `ContributionType`. They are defined, exported, but cannot be registered — dead code that adds confusion.

#### 8. `LLMProviderRegistrar` and `FormatterRegistrar` are nearly identical

**File**: `packages/sdk/plugin/contributions/registrar.ts:40-50`

Both register a `PluginLLMFormatter` as `BaseFormatter` with identical implementation. The semantic distinction between "LLM provider formatting" and "general formatter" is unclear and adds unnecessary complexity.

#### 9. Redundant `(this as any)._pluginEngine` assignment

**File**: `packages/sdk/api/shared/core/sdk-instance.ts:579`

```typescript
(this as any)._pluginEngine = pluginEngine;   // ❌ Redundant
(this as SDKInstance).pluginEngine = pluginEngine;  // ✅ Declared property
```

`pluginEngine` is already declared as a class property (line 65). The `(this as any)` assignment is unnecessary.

#### 10. `ContributionManagerClass` import alias is meaningless

**File**: `packages/sdk/di/container-config.ts:137`

```typescript
import { ContributionManager as ContributionManagerClass } from "../plugin/contributions/manager.js";
```

The alias is never used for disambiguation — the implementation just calls `new ContributionManagerClass()`, identical to `new ContributionManager()`.

#### 11. `require.cache` does not work in ESM context

**File**: `packages/sdk/plugin/loader.ts:117-127`

The `clearModuleCache()` method uses `require.cache` (CommonJS), but the project is `"type": "module"` (ESM). In ESM, `require` is unavailable unless `createRequire` is used. The hot-reload cache busting mechanism is effectively broken.

#### 12. `sdkVersion` hardcoded to `"1.0.0"`

**File**: `packages/sdk/api/shared/core/sdk-instance.ts:573`

```typescript
sdkVersion: "1.0.0", // TODO: get actual SDK version
```

This is a hardcoded placeholder. All plugin `sdkVersion` semver validation is effectively disabled until the actual SDK version is injected.

#### 13. Bridge hardcodes `CUSTOM_NODE` type

**File**: `packages/sdk/plugin/contributions/bridge.ts:225-236`

`createPluginNodeTemplate()` hardcodes `type: 'CUSTOM_NODE'`, but plugins may contribute arbitrary node types. `NodeTemplate['type']` is a union of `StaticNodeType` — `CUSTOM_NODE` may not be a valid member.

---

### 🔵 Architecture

#### 14. Missing consumer-side integration for ContributionManager

The current design only completes the "registration" path (plugin → ContributionManager). The "consumption" path (ContributionManager → execution engine) is missing. The execution engine should query `ContributionManager` as a fallback when a node type / tool type / LLM provider is not found in the standard registries.

#### 15. Bridge sync should be bidirectional or event-driven

The bridge currently only does a one-time "push" (sync to registries). Changes to registries (e.g., a tool deleted by another component) are not reflected back to the plugin. Consider a bidirectional sync or event-driven approach.

#### 16. No test coverage

The plugin module has no unit tests, integration tests, or type tests. For an 18-file, ~2700-line new module, this is a significant gap.

---

## 4. Summary

| Dimension | Assessment |
|---|---|
| **Architecture design** | Good. Clear separation of concerns, ISP applied well, well-defined lifecycle |
| **Feature completeness** | Good. Covers the full plugin lifecycle from discovery to deactivation |
| **Code quality** | Moderate. Some dead code, redundant assignments, ESM compatibility issues |
| **Integration integrity** | ⚠️ **Critical deficiency**. Bridge not wired to SDK registries — contributions invisible to the execution engine |
| **Testability** | Poor. No test coverage |

### Immediate priorities

1. Pass actual `SDKRegistries` to `PluginEngine` in `SDKInstance` so the bridge can function
2. Implement consumer-side query of `ContributionManager` in the execution engine
3. Fix `unsyncPluginContributions` to properly clean up tools and event handlers
4. Remove dead code from `abstractions.ts` or add corresponding `ContributionType` entries
5. Replace `require.cache` with ESM-compatible cache busting