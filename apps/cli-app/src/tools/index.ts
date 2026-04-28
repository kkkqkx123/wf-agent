/**
 * Tool module entry point
 *
 * Exports all tool-related types, registration center and tool definitions
 * Tool definitions are imported from SDK predefined resources, runtime management remains at the app layer
 */

// Type export (re-exporting from the SDK)
export type { ToolOutput } from "@wf-agent/types";

// App-specific type exports
export type { ToolRegistryConfig } from "./types.js";

// Export tool definition types from tool-executors
export type { ToolDefinitionLike } from "@wf-agent/tool-executors";

// Registration Center Export
export { ToolRegistry, createToolRegistry } from "./registry.js";

// Reusing components for exporting tool-executors
export {
  FunctionRegistry,
  StatelessExecutor,
  StatefulExecutor,
  TimeoutController,
  ParameterValidator,
  RetryStrategy,
  toSdkTool,
  toSdkTools,
} from "@wf-agent/tool-executors";

// Import predefined tools from the SDK.
export {
  createPredefinedTools,
  registerPredefinedTools,
  type PredefinedToolsOptions,
} from "@wf-agent/sdk";

// Auxiliary function exports (re-exported from common-utils)
export { resolvePath } from "./utils.js";
