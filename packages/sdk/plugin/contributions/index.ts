/**
 * Contributions Index - Public exports for the contribution system.
 */

export { ContributionManager } from "./manager.js";
export { ContributionRegistrarImpl } from "./registrar.js";
export type {
  NodeTypeRegistrar,
  ToolTypeRegistrar,
  LLMProviderRegistrar,
  FormatterRegistrar,
  EventHandlerRegistrar,
  HookHandlerRegistrar,
  MiddlewareRegistrar,
  ContributionRegistrar,
} from "./registrar.js";
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
} from "./abstractions.js";
export { isValidContributionType, validateContribution } from "./validation.js";
export type { MiddlewarePhase, ExecutionMiddleware } from "./middleware.types.js";
export type {
  ContributionType,
  IToolExecutorConstructor,
  HookHandler,
  EventHandler,
} from "./types.js";

// Re-export ContributionRegistrar from registrar.ts (the new combined interface)
export type { ContributionRegistrar } from "./registrar.js";