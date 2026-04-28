/**
 * Storage package logger
 * Lazy initialization mode - logger is only created on first access
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - STORAGE_*: Storage package specific settings
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
  return createPackageLogger("storage", {
    level: getLogLevelFromEnv("STORAGE_LOG_LEVEL", "GLOBAL_LOG_LEVEL", "info"),
    json: process.env["NODE_ENV"] === "production",
  });
}

/**
 * Package-level logger
 * Lazy initialization - only created on first access
 * Supports pre-configuration via configureLazyLogger before first use
 */
export const logger: Logger = createLazyLogger("storage", createLoggerInstance);

/**
 * Configure the logger before it's initialized
 * @param config Configuration options
 */
export function configureLogger(config: { level?: LogLevel }): void {
  configureLazyLogger("storage", config);
}

/**
 * Create a module-level logger
 * Child loggers inherit parent's stream and level, no need to register separately
 * @param moduleName Module name
 * @returns Module-level Logger instance
 */
export function createModuleLogger(moduleName: string): Logger {
  return logger.child(moduleName);
}
