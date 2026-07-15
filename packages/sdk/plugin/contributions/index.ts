/**
 * Contributions Index - Public exports for the contribution system.
 */

export { ContributionManager } from "./manager.js";
export { ContributionRegistrarImpl } from "./registration.js";
export type {
  NodeTypeRegistrar,
  ToolTypeRegistrar,
  LLMProviderRegistrar,
  FormatterRegistrar,
  EventHandlerRegistrar,
  HookHandlerRegistrar,
  MiddlewareRegistrar,
  ContributionRegistrar,
} from "./registration.js";
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

// Re-export ContributionRegistrar from registration.ts (plugin-facing adapter)
export type { ContributionRegistrar } from "./registration.js";