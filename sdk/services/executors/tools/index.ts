/**
 * Tool Executor Package
 * Provides implementations for various tool executors
 */

// Logger Export
export { logger, createModuleLogger } from "./logger.js";

// Core interfaces and base classes
export { IToolExecutor } from "./core/interfaces/IToolExecutor.js";
export { BaseExecutor } from "./core/base/BaseExecutor.js";
export { ParameterValidator } from "./core/base/ParameterValidator.js";
export { RetryStrategy } from "./core/base/RetryStrategy.js";
export { TimeoutController } from "./core/base/TimeoutController.js";
export { ToolType, ExecutorConfig, ExecutorMetadata } from "./core/types.js";

// REST Executor
export { RestExecutor } from "./rest/RestExecutor.js";
export type {
  HttpRequestConfig,
  HttpResponse,
  RequestInterceptor,
  ResponseInterceptor,
  ErrorInterceptor,
  RestExecutorConfig,
} from "./rest/types.js";

// Stateful Executor
export { StatefulExecutor } from "./stateful/StatefulExecutor.js";
export type { StatefulExecutorConfig } from "./stateful/types.js";

// Stateless executor
export { StatelessExecutor } from "./stateless/StatelessExecutor.js";
export { FunctionRegistry } from "./stateless/registry/FunctionRegistry.js";
export type { FunctionRegistryItem, FunctionRegistryConfig } from "./stateless/types.js";

// Builtin executor
export { BuiltinExecutor } from "./builtin/BuiltinExecutor.js";
export type { BuiltinExecutorConfig } from "./builtin/types.js";

// Auxiliary functions
export { toSdkTool, toSdkTools } from "./utils.js";
export type { ToolDefinitionLike } from "./utils.js";
