/**
 * Plugin Module - Public exports for the plugin system.
 *
 * The plugin system provides:
 * - PluginEngine: Central orchestrator for plugin lifecycle management
 * - PluginManifest, Plugin, PluginContext: Core plugin interfaces
 * - ContributionManager: Central manager for plugin contributions
 * - PluginRegistry: Registry for tracking plugin records
 * - PluginLoader: Discovery and loading of plugins
 * - PluginDependencyResolver: Inter-plugin dependency resolution
 * - PluginGuard: Stability guarantees (timeout + error isolation) for plugin execution
 */

// Core types
export type {
  PluginManifest,
  Plugin,
  PluginContext,
  PluginLogger,
  PluginRecord,
  ContributionRecord,
  DiscoveredPlugin,
  ValidationResult,
  PluginSystemOptions,
  PluginEngineOptions,
  ResolvedDependencyGraph,
} from "./types.js";

export { PluginStatus, type PluginLifecycleHook } from "./types.js";
export type { ContributionType } from "./contributions/types.js";

// Core engine
export { PluginEngine } from "./engine.js";

// Registry
export { PluginRegistry } from "./registry.js";

// Loader
export { PluginLoader, PluginLoadError } from "./loader.js";

// Dependency Resolver
export { PluginDependencyResolver } from "./dependency-resolver.js";

// Event Bus
export { PluginEventBus } from "./event-bus.js";
export type { PluginEventType, PluginEvent, PluginEventListener } from "./event-bus.js";

// Guard (replaces PluginSandbox)
export { PluginGuard, PluginGuardError } from "./guard.js";
export type { PluginGuardOptions } from "./guard.js";

// Configuration
export { OverridePolicy, mergePluginOptions, DEFAULT_PLUGIN_OPTIONS } from "./config.js";

// Contributions
export { ContributionManager } from "./contributions/manager.js";
export { isValidContributionType } from "./contributions/validation.js";

export type {
  ContributionRegistrar,
  NodeTypeRegistrar,
  ToolTypeRegistrar,
  LLMProviderRegistrar,
  FormatterRegistrar,
  EventHandlerRegistrar,
  HookHandlerRegistrar,
  MiddlewareRegistrar,
} from "./contributions/registration.js";

export type {
  PluginNodeHandler,
  PluginToolExecutor,
  PluginToolInstance,
  PluginLLMFormatter,
  PluginEventHandler,
  PluginHookHandler,
  PluginExecutionMiddleware,
  PluginExecutionContext,
  PluginNodeResult,
  PluginToolContext,
  PluginToolResult,
  PluginMessage,
  PluginLLMConfig,
  PluginLLMRequest,
  PluginLLMResponse,
} from "./contributions/abstractions.js";

export type { MiddlewarePhase, ExecutionMiddleware } from "./contributions/middleware.types.js";

export type {
  IToolExecutorConstructor,
  HookHandler,
  EventHandler,
} from "./contributions/types.js";