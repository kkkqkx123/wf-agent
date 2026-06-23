/**
 * Tool Executor Package
 *
 * Architecture:
 * - core/: Base interfaces and utilities for tool execution
 * - executors/: Concrete implementations (REST, Stateless, Stateful, Builtin, MCP)
 *
 * Responsibility: Execute various types of tools (REST APIs, JavaScript, Python, etc.)
 * Does NOT include tool management/registration (see core/registry/tool-registry.ts)
 */

// ============================================================================
// Core Interfaces & Base Classes
// ============================================================================
export { IToolExecutor } from "@sdk/services/tools/core/interfaces.js";
export {
  BaseExecutor,
  ParameterValidator,
  RetryStrategy,
  TimeoutController,
  type RetryStrategyConfig,
} from "@sdk/services/tools/core/base.js";
export { ToolType, ExecutorConfig, ExecutorMetadata } from "@sdk/services/tools/core/types.js";

// ============================================================================
// Tool Executor Implementations
// ============================================================================

// REST Executor - HTTP/HTTPS API calls
export { RestExecutor } from "./executors/rest.js";
export type {
  HttpRequestConfig,
  HttpResponse,
  RequestInterceptor,
  ResponseInterceptor,
  ErrorInterceptor,
  RestExecutorConfig,
} from "./executors/rest.js";

// Stateful Executor - Maintains state across calls
export { StatefulExecutor } from "./executors/stateful.js";
export type { StatefulExecutorConfig } from "./executors/stateful.js";

// Stateless Executor - JavaScript function registry
export { StatelessExecutor } from "./executors/stateless.js";
export type { FunctionRegistryItem, FunctionRegistryConfig } from "./executors/stateless.js";

// Builtin Executor - Built-in tools
export { BuiltinExecutor } from "./executors/builtin.js";
export type { BuiltinExecutorConfig } from "./executors/builtin.js";

// MCP Executor - Model Context Protocol servers
export { McpExecutor } from "./executors/mcp.js";
export type { McpToolConfig } from "./executors/mcp.js";

// ============================================================================
// Utilities
// ============================================================================
export { logger, createModuleLogger } from "./logger.js";
export { toSdkTool, toSdkTools } from "./utils.js";
export type { ToolDefinitionLike } from "./utils.js";
