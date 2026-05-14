/**
 * Tool Executor Package Logger
 * Lazy initialization mode - logger is only created on first access
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - TOOL_EXECUTORS_*: Tool executors package specific settings
 */

import {
  createPackageLogger,
  createLazyLogger,
  configureLazyLogger,
  getLogLevelFromEnv,
} from "@wf-agent/common-utils";
import type { Logger, LogLevel } from "@wf-agent/common-utils";

/**
 * Factory function to create the actual logger instance
 */
function createLoggerInstance(): Logger {
  return createPackageLogger("tool-executors", {
    level: getLogLevelFromEnv("TOOL_EXECUTORS_LOG_LEVEL", "GLOBAL_LOG_LEVEL", "info"),
    json: process.env["NODE_ENV"] === "production",
  });
}

/**
 * Package-level logger
 * Lazy initialization - only created on first access
 * Supports pre-configuration via configureLazyLogger before first use
 */
export const logger: Logger = createLazyLogger("tool-executors", createLoggerInstance);

/**
 * Configure the logger before it's initialized
 * @param config Configuration options
 */
export function configureLogger(config: { level?: LogLevel }): void {
  configureLazyLogger("tool-executors", config);
}

/**
 * Create a module-level logger
 * Child loggers inherit parent's stream and level, no need to register separately
 * @param moduleName Module name
 * @returns An instance of the module-level Logger
 */
export function createModuleLogger(moduleName: string): Logger {
  return logger.child(moduleName);
}
