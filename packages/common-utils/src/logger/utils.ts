/**
 * Log Tool Functions
 * A performance optimization tool based on the design philosophy of Pino
 */

import type { LogLevel, LoggerContext, LogEntry } from "./types.js";
import { LOG_LEVEL_PRIORITY, LOG_SCHEMA_VERSION } from "./types.js";
import { getContextAsLoggerContext } from "./async-context.js";

/**
 * Check whether the log level should be printed out
 * @param currentLevel The currently configured log level
 * @param messageLevel The log level of the message
 * @returns Whether the log should be printed out
 */
export function shouldLog(currentLevel: LogLevel, messageLevel: LogLevel): boolean {
  return LOG_LEVEL_PRIORITY[messageLevel] >= LOG_LEVEL_PRIORITY[currentLevel];
}

/**
 * Format the timestamp
 * @returns A timestamp in ISO format
 */
export function formatTimestamp(): string {
  return new Date().toISOString();
}

/**
 * Merge context objects
 * @param base: Base context
 * @param additional: Additional context
 * @returns: Merged context
 */
export function mergeContext(base: LoggerContext, additional: LoggerContext = {}): LoggerContext {
  return { ...base, ...additional };
}

/**
 * Create a log entry with standard schema
 * Optimized to reduce AsyncLocalStorage access
 * @param level Log level
 * @param message Log message
 * @param context Log context
 * @param timestamp Whether to include a timestamp
 * @param loggerName Logger name
 * @returns Log entry following standard schema
 */
export function createLogEntry(
  level: LogLevel,
  message: string,
  context?: LoggerContext,
  timestamp: boolean = true,
  loggerName?: string,
): LogEntry {
  const entry: LogEntry = {
    level,
    message,
    v: LOG_SCHEMA_VERSION, // Schema version for compatibility
  };

  if (timestamp) {
    entry.timestamp = formatTimestamp();
  }

  if (loggerName) {
    entry.logger = loggerName;
  }

  // Get async context once and extract all needed values
  const asyncContext = getContextAsLoggerContext();

  // Extract tracing information from async context
  if (asyncContext["traceId"]) {
    entry.traceId = asyncContext["traceId"] as string;
  }
  if (asyncContext["spanId"]) {
    entry.spanId = asyncContext["spanId"] as string;
  }

  // Merge with provided context
  const mergedContext = { ...asyncContext, ...context };
  // Remove tracing keys from context (they're already set at top level)
  delete mergedContext["traceId"];
  delete mergedContext["spanId"];

  if (Object.keys(mergedContext).length > 0) {
    entry.context = mergedContext;
  }

  return entry;
}
