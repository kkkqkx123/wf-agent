/**
 * The script executor package exports everything in a unified manner.
 */

// Logger Export
export { logger, createModuleLogger } from "./logger.js";

// Core Interfaces and Types
export { IScriptExecutor } from "./core/interfaces/IScriptExecutor.js";
export type {
  ExecutorType,
  ExecutorConfig,
  ExecutionContext,
  ExecutionOutput,
  ValidationResult,
  ExecutorMetadata,
} from "./core/types.js";

// Base classes and components
export { BaseScriptExecutor } from "./core/base/BaseScriptExecutor.js";
export { CommandLineExecutor } from "./core/base/CommandLineExecutor.js";
export { RetryStrategy } from "./core/base/RetryStrategy.js";
export { TimeoutController } from "./core/base/TimeoutController.js";
export type { CommandLineConfig } from "./core/base/CommandLineExecutor.js";

// Specific executor
export { ShellExecutor } from "./shell/ShellExecutor.js";
export { PythonExecutor } from "./python/PythonExecutor.js";
export { JavaScriptExecutor } from "./javascript/JavaScriptExecutor.js";
export { PowerShellExecutor } from "./powershell/PowerShellExecutor.js";
export { CmdExecutor } from "./cmd/CmdExecutor.js";
