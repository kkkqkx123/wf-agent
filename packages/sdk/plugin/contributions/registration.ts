/**
 * Contribution Registration Interfaces - Plugin-agnostic registration API.
 *
 * These interfaces define the contract between plugins and the SDK contribution system.
 * They are the only types that plugins need to import for registering contributions.
 * Implementation is provided by ContributionManager.
 */

import type {
  PluginNodeHandler,
  PluginToolExecutor,
  PluginLLMFormatter,
  PluginEventHandler,
  PluginHookHandler,
  PluginExecutionMiddleware,
} from "./abstractions.js";

// ============================================================
// Sub-Registrar Interfaces
// ============================================================

export interface NodeTypeRegistrar {
  registerNodeType(type: string, handler: PluginNodeHandler): void;
}

export interface ToolTypeRegistrar {
  registerToolType(type: string, executor: PluginToolExecutor): void;
}

export interface LLMProviderRegistrar {
  registerLLMProvider(provider: string, formatter: PluginLLMFormatter): void;
}

export interface FormatterRegistrar {
  registerFormatter(name: string, formatter: PluginLLMFormatter): void;
}

export interface EventHandlerRegistrar {
  registerEventHandler(eventType: string, handler: PluginEventHandler): void;
}

export interface HookHandlerRegistrar {
  registerHookHandler(hookType: string, handler: PluginHookHandler): void;
}

export interface MiddlewareRegistrar {
  registerMiddleware(phase: string, middleware: PluginExecutionMiddleware): void;
}

// ============================================================
// Combined Registrar Interface
// ============================================================

/**
 * ContributionRegistrar - Combined interface for registering all contribution types.
 *
 * Plugins receive this interface via the `registerContributions` lifecycle hook.
 * Each sub-registrar is lazily available and may be `undefined` if the
 * contribution type is not enabled in the plugin system configuration.
 */
export interface ContributionRegistrar {
  readonly nodeTypes: NodeTypeRegistrar | undefined;
  readonly toolTypes: ToolTypeRegistrar | undefined;
  readonly llmProviders: LLMProviderRegistrar | undefined;
  readonly formatters: FormatterRegistrar | undefined;
  readonly eventHandlers: EventHandlerRegistrar | undefined;
  readonly hookHandlers: HookHandlerRegistrar | undefined;
  readonly middleware: MiddlewareRegistrar | undefined;
}