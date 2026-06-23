/**
 * Base Logger Implementation
 * Core logger class supporting child logger pattern and stream output
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
import { LogRateLimiter } from "./rate-limiter.js";

/**
 * Basic Logger Implementation
 * Supports child logger mode and stream output
 */
export class BaseLogger implements Logger {
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
