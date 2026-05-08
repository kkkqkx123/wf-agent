/**
 * Lazy Logger System
 * Proxy-based lazy initialization to avoid side effects during module load
 */

import type { Logger, LogLevel, LogStream } from "./types.js";
import { loggerRegistry } from "./logger-registry.js";

/**
 * Factory function type for creating loggers
 */
export type LoggerFactory = () => Logger;

/**
 * Lazy logger instance cache
 */
const lazyLoggerCache: Map<string, Logger> = new Map();

/**
 * Pending configuration for lazy loggers
 * Stores config to be applied when logger is first accessed
 */
const lazyLoggerPendingConfig: Map<string, { level?: LogLevel; stream?: LogStream }> = new Map();

/**
 * Create a lazy logger using Proxy
 * The actual logger is only created on first access
 * @param name Logger name (for registration and caching)
 * @param factory Factory function that creates the actual logger
 * @returns Proxy-based lazy logger
 */
export function createLazyLogger(name: string, factory: LoggerFactory): Logger {
  return new Proxy({} as Logger, {
    get(_, prop: string | symbol) {
      // Get or create the actual logger instance
      let instance = lazyLoggerCache.get(name);
      if (!instance) {
        instance = factory();
        lazyLoggerCache.set(name, instance);

        // Apply any pending configuration
        const pending = lazyLoggerPendingConfig.get(name);
        if (pending) {
          if (pending.level) instance.setLevel(pending.level);
          if (pending.stream && instance.setStream) {
            instance.setStream(pending.stream);
          }
          lazyLoggerPendingConfig.delete(name);
        }

        // Register to global registry
        loggerRegistry.set(name, instance);
      }

      const value = (instance as unknown as Record<string, unknown>)[prop as string];
      if (typeof value === "function") {
        return value.bind(instance);
      }
      return value;
    },
  });
}

/**
 * Configure a lazy logger before it's initialized
 * @param name Logger name
 * @param config Configuration to apply
 */
export function configureLazyLogger(
  name: string,
  config: { level?: LogLevel; stream?: LogStream },
): void {
  const instance = lazyLoggerCache.get(name);
  if (instance) {
    // Logger already created, apply config directly
    if (config.level) instance.setLevel(config.level);
    if (config.stream && instance.setStream) {
      instance.setStream(config.stream);
    }
  } else {
    // Store config for when logger is created
    lazyLoggerPendingConfig.set(name, config);
  }
}

/**
 * Check if a lazy logger has been initialized
 * @param name Logger name
 * @returns True if initialized
 */
export function isLazyLoggerInitialized(name: string): boolean {
  return lazyLoggerCache.has(name);
}

/**
 * Get lazy logger instance if initialized
 * @param name Logger name
 * @returns Logger instance or undefined
 */
export function getLazyLoggerInstance(name: string): Logger | undefined {
  return lazyLoggerCache.get(name);
}
