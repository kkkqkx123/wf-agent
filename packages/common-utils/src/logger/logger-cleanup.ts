/**
 * Logger System Cleanup and Reset Utilities
 * Provides functions to clean up logger state for testing and re-initialization scenarios
 */

import { lazyLoggerCache, lazyLoggerPendingConfig } from "./lazy-logger.js";
import { loggerRegistry } from "./logger-registry.js";
import { setGlobalLogger } from "./global-logger.js";
import { BaseLogger } from "./base-logger.js";

/**
 * Clear all lazy logger caches
 * Use this in test environments to reset state between tests
 */
export function clearLazyLoggerCache(): void {
  lazyLoggerCache.clear();
  lazyLoggerPendingConfig.clear();
}

/**
 * Clear the global logger registry
 * WARNING: This will remove all registered loggers. Use with caution.
 */
export function clearLoggerRegistry(): void {
  loggerRegistry.clear();
}

/**
 * Reset the global logger to default
 */
export function resetGlobalLogger(): void {
  setGlobalLogger(new BaseLogger({ level: "info" }));
}

/**
 * Reset all logger system state to initial values
 * This is primarily intended for testing purposes
 * 
 * WARNING: Do not use in production code as it will:
 * - Remove all registered loggers
 * - Clear all lazy logger configurations
 * - Reset the global logger
 * - NOT remove process exit handlers (they cannot be unregistered safely)
 */
export function resetLoggerSystem(): void {
  clearLazyLoggerCache();
  clearLoggerRegistry();
  resetGlobalLogger();
}

/**
 * Get current logger system statistics (for debugging/testing)
 */
export function getLoggerSystemStats(): {
  lazyLoggerCount: number;
  pendingConfigCount: number;
  registeredLoggerCount: number;
} {
  return {
    lazyLoggerCount: lazyLoggerCache.size,
    pendingConfigCount: lazyLoggerPendingConfig.size,
    registeredLoggerCount: loggerRegistry.size,
  };
}
