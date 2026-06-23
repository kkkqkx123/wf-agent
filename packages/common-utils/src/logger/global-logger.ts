/**
 * Global Logger Management
 * Manages the global logger instance and provides convenience functions
 */

import type { Logger, LogLevel } from "./types.js";
import { BaseLogger } from "./base-logger.js";

// Global Logger Instance
let globalLogger: Logger = new BaseLogger({ level: "info" });

/**
 * Set the global logger
 * @param logger Logger instance
 */
export function setGlobalLogger(logger: Logger): void {
  globalLogger = logger;
}

/**
 * Get the global logger
 * @returns Global Logger instance
 */
export function getGlobalLogger(): Logger {
  return globalLogger;
}

/**
 * Set the global log level
 * @param level Log level
 */
export function setGlobalLogLevel(level: LogLevel): void {
  if (globalLogger instanceof BaseLogger) {
    globalLogger.setLevel(level);
  } else {
    // If the global logger is not an instance of BaseLogger, create a new one.
    globalLogger = new BaseLogger({ level });
  }
}

/**
 * Get the global log level
 * @returns The current global log level
 */
export function getGlobalLogLevel(): LogLevel {
  return globalLogger.getLevel();
}
