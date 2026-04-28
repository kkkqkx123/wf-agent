/**
 * Core Logger Implementation
 * A streaming logging system based on the Pino design philosophy
 * Supports the child logger pattern and various output methods
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - SDK_*: SDK module specific settings
 * - SDK_*_GRAPH: SDK Graph submodule settings
 * - SDK_*_AGENT: SDK Agent submodule settings
 */

import type {
  Logger,
  LogLevel,
  LoggerContext,
  LoggerOptions,
  LogStream,
  LogEntry,
} from "./types.js";
import { createConsoleStream } from "./streams/index.js";
import { shouldLog, mergeContext, createLogEntry } from "./utils.js";

// Environment Variable Names (Layered Architecture)
const ENV_VARS = {
  // Global settings
  GLOBAL_LOG_LEVEL: "GLOBAL_LOG_LEVEL",

  // SDK module specific
  SDK_LOG_LEVEL: "SDK_LOG_LEVEL",
  SDK_LOG_LEVEL_GRAPH: "SDK_LOG_LEVEL_GRAPH",
  SDK_LOG_LEVEL_AGENT: "SDK_LOG_LEVEL_AGENT",
} as const;

/**
 * Get log level from environment variable with fallback chain
 * Priority: primaryKey > globalKey > defaultLevel
 *
 * This is the unified utility function for reading log levels from environment variables.
 * Packages should use this instead of implementing their own logic.
 *
 * @param primaryKey - Primary environment variable key (package-specific)
 * @param globalKey - Global fallback key (defaults to GLOBAL_LOG_LEVEL)
 * @param defaultLevel - Default level if neither env var is set
 * @returns The resolved log level
 *
 * @example
 * // In a package logger:
 * const level = getLogLevelFromEnv(
 *   "SCRIPT_EXECUTORS_LOG_LEVEL",
 *   "GLOBAL_LOG_LEVEL",
 *   "info"
 * );
 */
export function getLogLevelFromEnv(
  primaryKey: string,
  globalKey: string = ENV_VARS.GLOBAL_LOG_LEVEL,
  defaultLevel: LogLevel = "info",
): LogLevel {
  const value = process.env[primaryKey] as LogLevel | undefined;
  if (value) return value;
  const globalValue = process.env[globalKey] as LogLevel | undefined;
  if (globalValue) return globalValue;
  return defaultLevel;
}

/**
 * Get default log level from environment variables
 * Priority: SDK_LOG_LEVEL > GLOBAL_LOG_LEVEL > info
 */
function _getDefaultLogLevel(): LogLevel {
  return getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL, ENV_VARS.GLOBAL_LOG_LEVEL, "info");
}

/**
 * Get log level for graph module from environment variables
 * Priority: SDK_LOG_LEVEL_GRAPH > SDK_LOG_LEVEL > GLOBAL_LOG_LEVEL > info
 */
function _getGraphLogLevel(): LogLevel {
  return getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL_GRAPH, ENV_VARS.GLOBAL_LOG_LEVEL, "info");
}

/**
 * Get log level for agent module from environment variables
 * Priority: SDK_LOG_LEVEL_AGENT > SDK_LOG_LEVEL > GLOBAL_LOG_LEVEL > info
 */
function _getAgentLogLevel(): LogLevel {
  return getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL_AGENT, ENV_VARS.GLOBAL_LOG_LEVEL, "info");
}

/**
 * Rate limiter for log throttling
 */
class LogRateLimiter {
  private maxLogsPerSecond: number;
  private logCount: number = 0;
  private lastResetTime: number = Date.now();
  private droppedCount: number = 0;

  constructor(maxLogsPerSecond: number) {
    this.maxLogsPerSecond = maxLogsPerSecond;
  }

  /**
   * Check if a log should be allowed
   * Returns true if allowed, false if rate limited
   */
  allow(): boolean {
    const now = Date.now();
    if (now - this.lastResetTime >= 1000) {
      // Reset counter every second
      if (this.droppedCount > 0) {
        // Log how many were dropped (once per second)
        // eslint-disable-next-line no-console
        console.warn(
          `[Logger] Rate limit exceeded, dropped ${this.droppedCount} logs in last second`,
        );
        this.droppedCount = 0;
      }
      this.logCount = 0;
      this.lastResetTime = now;
    }

    if (this.logCount < this.maxLogsPerSecond) {
      this.logCount++;
      return true;
    }

    this.droppedCount++;
    return false;
  }
}

/**
 * Basic Logger Implementation
 * Supports child logger mode and stream output
 */
class BaseLogger implements Logger {
  protected level: LogLevel;
  protected context: LoggerContext;
  protected stream: LogStream;
  protected name?: string;
  protected timestamp: boolean;
  protected sampleRate: number;
  protected rateLimiter?: LogRateLimiter;
  protected category?: string;
  protected tags?: string[];

  constructor(options: LoggerOptions = {}, parentContext: LoggerContext = {}) {
    this.level = options.level || "info";
    this.name = options.name;
    this.context = { ...parentContext, ...options.base };
    this.timestamp = options.timestamp !== false; // The default includes a timestamp.
    this.sampleRate = Math.max(0, Math.min(1, options.sampleRate ?? 1.0)); // Clamp between 0-1
    this.category = options.category;
    this.tags = options.tags;

    // Initialize rate limiter if configured
    if (options.maxLogsPerSecond && options.maxLogsPerSecond > 0) {
      this.rateLimiter = new LogRateLimiter(options.maxLogsPerSecond);
    }

    // Create or use the provided stream.
    if (options.stream) {
      this.stream = options.stream;
    } else {
      // Create a default console stream
      this.stream = createConsoleStream({
        json: options.json ?? false,
        timestamp: this.timestamp,
        pretty: options.pretty ?? false,
      });
    }
  }

  /**
   * Check if this log entry should be sampled
   * Based on sampleRate configuration
   */
  protected shouldSample(): boolean {
    if (this.sampleRate >= 1.0) return true;
    return Math.random() < this.sampleRate;
  }

  /**
   * Check if this log entry passes rate limiting
   */
  protected checkRateLimit(): boolean {
    if (!this.rateLimiter) return true;
    return this.rateLimiter.allow();
  }

  /**
   * Set the log level
   */
  setLevel(level: LogLevel): void {
    this.level = level;
  }

  /**
   * Get the current log level
   */
  getLevel(): LogLevel {
    return this.level;
  }

  /**
   * Check whether the specified level is enabled.
   */
  isLevelEnabled(level: LogLevel): boolean {
    return shouldLog(this.level, level);
  }

  /**
   * Check if log should be written (level + sampling + rate limit)
   */
  protected shouldWrite(level: LogLevel): boolean {
    // Check log level first
    if (!this.isLevelEnabled(level)) {
      return false;
    }
    // Check sampling (errors are always logged regardless of sampling)
    if (level !== "error" && !this.shouldSample()) {
      return false;
    }
    // Check rate limiting (errors bypass rate limiting)
    if (level !== "error" && !this.checkRateLimit()) {
      return false;
    }
    return true;
  }

  /**
   * Create log entry with logger name
   */
  protected createEntry(level: LogLevel, message: string, context?: Record<string, unknown>): LogEntry {
    const mergedContext = mergeContext(this.context, context);
    const entry = createLogEntry(level, message, mergedContext, this.timestamp, this.name);

    // Add sampling flag if sampling is enabled
    if (this.sampleRate < 1.0) {
      entry.sampled = true;
    }

    // Add category from logger config or context
    const contextCategory = context?.["category"];
    if (typeof contextCategory === "string") {
      entry.category = contextCategory;
    } else if (this.category) {
      entry.category = this.category;
    }

    // Add tags from logger config and context
    const contextTags = context?.["tags"];
    const normalizedContextTags = Array.isArray(contextTags) ? contextTags : [];
    const loggerTags = this.tags ?? [];
    if (normalizedContextTags.length > 0 || loggerTags.length > 0) {
      entry.tags = [...new Set([...loggerTags, ...normalizedContextTags])];
    }

    return entry;
  }

  /**
   * Debug level logs
   */
  debug(message: string, context?: Record<string, unknown>): void {
    if (this.shouldWrite("debug")) {
      const entry = this.createEntry("debug", message, context);
      this.stream.write(entry);
    }
  }

  /**
   * Information level logs
   */
  info(message: string, context?: Record<string, unknown>): void {
    if (this.shouldWrite("info")) {
      const entry = this.createEntry("info", message, context);
      this.stream.write(entry);
    }
  }

  /**
   * Warning level logs
   */
  warn(message: string, context?: Record<string, unknown>): void {
    if (this.shouldWrite("warn")) {
      const entry = this.createEntry("warn", message, context);
      this.stream.write(entry);
    }
  }

  /**
   * Error level logs
   */
  error(message: string, context?: Record<string, unknown>): void {
    if (this.shouldWrite("error")) {
      const entry = this.createEntry("error", message, context);
      this.stream.write(entry);
    }
  }

  /**
   * Create a sub-logger
   * Similar to the child logger pattern in pino
   * @param name Name of the sub-logger
   * @param additionalContext Additional context information
   * @returns Instance of the sub-logger
   */
  child(name: string, additionalContext: LoggerContext = {}): Logger {
    const childOptions: LoggerOptions = {
      level: this.level,
      name: this.name ? `${this.name}.${name}` : name,
      stream: this.stream, // Share the same stream
      timestamp: this.timestamp,
      sampleRate: this.sampleRate, // Inherit sampling rate
      category: this.category, // Inherit category
      tags: this.tags, // Inherit tags
      // Note: rate limiter is not inherited, child loggers have their own limit if configured
    };

    const childContext = mergeContext(this.context, {
      module: name,
      ...additionalContext,
    });

    return new BaseLogger(childOptions, childContext);
  }

  /**
   * Flush the log buffer
   */
  flush(callback?: () => void): void {
    if (this.stream.flush) {
      this.stream.flush(callback);
    } else if (callback) {
      setImmediate(callback);
    }
  }

  /**
   * Replace the log stream at runtime
   * Used to redirect logs to different outputs after initialization
   * @param stream New log stream
   */
  setStream(stream: LogStream): void {
    // Flush any pending logs to the old stream
    this.flush();
    this.stream = stream;
  }
}

/**
 * Empty Operation Logger
 * Used to disable log output
 */
class NoopLogger implements Logger {
  debug(): void {}
  info(): void {}
  warn(): void {}
  error(): void {}

  child(): Logger {
    return this;
  }

  setLevel(): void {}
  getLevel(): LogLevel {
    return "off";
  }

  isLevelEnabled(): boolean {
    return false;
  }
}

// Global Logger Instance
let globalLogger: Logger = new BaseLogger({ level: "info" });

/**
 * Create a logger instance
 * @param options Logger configuration options
 * @returns Logger instance
 */
export function createLogger(options: LoggerOptions = {}): Logger {
  return new BaseLogger(options);
}

/**
 * Create a package-level logger
 * This is the recommended way to create a logger:
 * @param pkg Package name
 * @param options Logger configuration options
 * @returns Logger instance
 */
export function createPackageLogger(
  pkg: string,
  options: Omit<LoggerOptions, "name"> = {},
): Logger {
  const packageOptions: LoggerOptions = {
    ...options,
    name: pkg,
  };

  const packageContext: LoggerContext = {
    pkg,
  };

  return new BaseLogger(packageOptions, packageContext);
}

// ============================================
// Lazy Logger - Proxy-based lazy initialization
// Used to avoid side effects during module load
// ============================================

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
 * Factory function type for creating loggers
 */
export type LoggerFactory = () => Logger;

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

/**
 * Create a default console logger
 * @param level Log level, default is 'info'
 * @returns Logger instance
 */
export function createConsoleLogger(level: LogLevel = "info"): Logger {
  return new BaseLogger({ level });
}

/**
 * Create an empty operation logger
 * Used to disable log output
 * @returns Logger instance
 */
export function createNoopLogger(): Logger {
  return new NoopLogger();
}

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

// ============================================
// Global Log Registry
// Used for unified control of the log levels for all packages
// ============================================

/**
 * Global Log Registry
 * Stores all registered package-level loggers
 */
const loggerRegistry: Map<string, Logger> = new Map();

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
    if (globalLogger) {
      globalLogger.error("Uncaught exception", { error: err.message, stack: err.stack });
    }
    flushAllLoggersSync();
    process.exit(1);
  });

  // Handle unhandled promise rejections
  process.on("unhandledRejection", (reason) => {
    if (globalLogger) {
      globalLogger.error("Unhandled promise rejection", { reason });
    }
    flushAllLoggersSync();
    process.exit(1);
  });
}
