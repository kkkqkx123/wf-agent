/**
 * Logger Registry and Process Management
 * Provides global registry for all loggers and process exit handlers
 */

import type { Logger, LogLevel } from "./types.js";
import { BaseLogger } from "./base-logger.js";
import { getGlobalLogger } from "./global-logger.js";

// ============================================
// Global Log Registry
// Used for unified control of the log levels for all packages
// ============================================

/**
 * Global Log Registry
 * Stores all registered package-level loggers
 */
export const loggerRegistry: Map<string, Logger> = new Map();

/**
 * Register the Logger in the global registry
 * @param name The name of the logger (usually the package name)
 * @param logger The Logger instance
 */
export function registerLogger(name: string, logger: Logger): void {
  loggerRegistry.set(name, logger);
}

/**
 * Log out from the Logger
 * @param name Name of the logger
 */
export function unregisterLogger(name: string): void {
  loggerRegistry.delete(name);
}

/**
 * Get the registered Logger
 * @param name Logger name
 * @returns Logger instance; returns undefined if not registered
 */
export function getRegisteredLogger(name: string): Logger | undefined {
  return loggerRegistry.get(name);
}

/**
 * Get all registered Logger names
 * @returns Array of Logger names
 */
export function getRegisteredLoggerNames(): string[] {
  return Array.from(loggerRegistry.keys());
}

/**
 * Set the log level for the specified Logger
 * Wildcard patterns are supported; for example, 'script-executors.*' can match 'script-executors' and all its sub-modules
 * @param name: Logger name (wildcards are allowed)
 * @param level: Log level
 */
export function setLoggerLevel(name: string, level: LogLevel): void {
  // Check if it is a wildcard pattern.
  if (name.endsWith(".*")) {
    const prefix = name.slice(0, -2); // Remove '.*'
    for (const [loggerName, logger] of loggerRegistry) {
      if (loggerName === prefix || loggerName.startsWith(`${prefix}.`)) {
        logger.setLevel(level);
      }
    }
  } else {
    const logger = loggerRegistry.get(name);
    if (logger) {
      logger.setLevel(level);
    }
  }
}

/**
 * Set the log level for all registered Loggers
 * @param level Log level
 */
export function setAllLoggersLevel(level: LogLevel): void {
  for (const logger of loggerRegistry.values()) {
    logger.setLevel(level);
  }
}

// ============================================
// Process Exit Handler
// Ensure all logs are flushed before process exit
// ============================================

/**
 * Flush all registered loggers synchronously
 * Used before process exit to ensure no logs are lost
 */
export function flushAllLoggersSync(): void {
  for (const logger of loggerRegistry.values()) {
    if (logger instanceof BaseLogger) {
      const stream = (logger as BaseLogger)["stream"];
      if (stream && "flushSync" in stream && typeof stream.flushSync === "function") {
        stream.flushSync();
      }
    }
  }
}

/**
 * Flush all registered loggers asynchronously
 * @param callback Completion callback
 */
export function flushAllLoggers(callback?: () => void): void {
  const loggers = Array.from(loggerRegistry.values());
  let pending = loggers.length;

  if (pending === 0) {
    if (callback) {
      setImmediate(callback);
    }
    return;
  }

  for (const logger of loggers) {
    if (logger instanceof BaseLogger) {
      logger.flush(() => {
        pending--;
        if (pending === 0 && callback) {
          callback();
        }
      });
    } else {
      pending--;
      if (pending === 0 && callback) {
        callback();
      }
    }
  }
}

/**
 * Setup process exit handlers to flush logs
 * Should be called once at application startup
 */
export function setupExitHandlers(): void {
  // Handle normal exit
  process.on("exit", () => {
    flushAllLoggersSync();
  });

  // Handle signals
  const signals = ["SIGINT", "SIGTERM", "SIGUSR2"];
  signals.forEach(signal => {
    process.on(signal, () => {
      flushAllLoggers(() => {
        process.exit(0);
      });
    });
  });

  // Handle uncaught exceptions
  process.on("uncaughtException", err => {
    // Try to log the error
    const globalLogger = getGlobalLogger();
    if (globalLogger) {
      globalLogger.error("Uncaught exception", { error: err.message, stack: err.stack });
    }
    flushAllLoggersSync();
    process.exit(1);
  });

  // Handle unhandled promise rejections
  process.on("unhandledRejection", (reason) => {
    const globalLogger = getGlobalLogger();
    if (globalLogger) {
      globalLogger.error("Unhandled promise rejection", { reason });
    }
    flushAllLoggersSync();
    process.exit(1);
  });
}
