/**
 * Script Executor Package Logger
 * Lazy initialization mode - logger is only created on first access
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - SCRIPT_EXECUTORS_*: Script executors package specific settings
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
  return createPackageLogger("script-executors", {
    level: getLogLevelFromEnv("SCRIPT_EXECUTORS_LOG_LEVEL", "GLOBAL_LOG_LEVEL", "info"),
    json: process.env["NODE_ENV"] === "production",
  });
}

/**
 * Package level logger
 * Lazy initialization - only created on first access
 * Supports pre-configuration via configureLazyLogger before first use
 */
export const logger: Logger = createLazyLogger("script-executors", createLoggerInstance);

/**
 * Configure the logger before it's initialized
 * @param config Configuration options
 */
export function configureLogger(config: { level?: LogLevel }): void {
  configureLazyLogger("script-executors", config);
}

/**
 * Creating a module-level logger
 * Child loggers inherit parent's stream and level, no need to register separately
 * @param moduleName module name
 * @returns Module-level Logger instance
 */
export function createModuleLogger(moduleName: string): Logger {
  return logger.child(moduleName);
}
