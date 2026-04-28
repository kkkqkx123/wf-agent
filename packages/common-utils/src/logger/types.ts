/**
 * Logging module type definition
 * Streaming logging system based on pino design ideas
 */

/**
 * Log level
 */
export type LogLevel = "debug" | "info" | "warn" | "error" | "off";

/**
 * Log Level Priority Mapping
 * Used to compare log levels
 */
export const LOG_LEVEL_PRIORITY: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
  off: 4,
};

/**
 * Log Context
 * Used to store meta information such as package name, module name, etc.
 */
export interface LoggerContext {
  pkg?: string;
  module?: string;
  [key: string]: unknown;
}

/**
 * Standard Log Entry Schema
 * Unified data format for all log outputs
 */
export interface LogEntry {
  /** Log level */
  level: LogLevel;

  /** Log message */
  message: string;

  /** ISO 8601 timestamp */
  timestamp?: string;

  /** Logger name (e.g., package name or module name) */
  logger?: string;

  /** Trace ID for distributed tracing */
  traceId?: string;

  /** Span ID for distributed tracing */
  spanId?: string;

  /** Parent span ID for distributed tracing */
  parentSpanId?: string;

  /** Context information */
  context?: LoggerContext;

  /** Sampling flag (true if this log was sampled) */
  sampled?: boolean;

  /** Log category for filtering (e.g., 'performance', 'security', 'business') */
  category?: string;

  /** Log tags for multi-dimensional filtering */
  tags?: string[];

  /** Schema version for compatibility */
  v?: string;

  /** Additional structured metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Standard Log Schema Version
 * Used for log format versioning and compatibility
 */
export const LOG_SCHEMA_VERSION = "1.0";

/**
 * LogStream Interface
 * Unified log output abstraction
 */
export interface LogStream {
  /**
   * Writing log entries
   * @param entry Log entry
   */
  write(entry: LogEntry): void;

  /**
   * Refresh buffer (optional)
   * @param callback Completion callback
   */
  flush?(callback?: () => void): void;

  /**
   * End stream (optional)
   */
  end?(): void;

  /**
   * Event listener (optional)
   * @param event Event name
   * @param handler Event handler
   */
  on?(event: string, handler: (...args: unknown[]) => void): void;

  /**
   * Remove event listener (optional)
   * @param event Event name
   * @param handler Event handler
   */
  off?(event: string, handler: (...args: unknown[]) => void): void;
}

/**
 * Stream Configuration Options
 */
export interface StreamOptions {
  /**
   * Whether to use JSON format output
   */
  json?: boolean;

  /**
   * Whether to include a timestamp
   */
  timestamp?: boolean;

  /**
   * Whether to use color output (console only)
   */
  pretty?: boolean;

  /**
   * Batch size (async stream only)
   */
  batchSize?: number;

  /**
   * File path (file stream only)
   */
  filePath?: string;

  /**
   * Whether to append to a file (file stream only)
   */
  append?: boolean;
}

/**
 * Multistream Configuration
 */
export interface MultistreamOptions {
  /**
   * Whether or not to de-duplicate
   */
  dedupe?: boolean;

  /**
   * Custom level mapping
   */
  levels?: Record<string, number>;
}

/**
 * StreamEntry
 * A stream entry within a Multistream
 */
export interface StreamEntry {
  /**
   * Stream Example
   */
  stream: LogStream;

  /**
   * Log level
   */
  level?: LogLevel | number;

  /**
   * Level value
   */
  levelVal?: number;

  /**
   * Stream ID (for removal)
   */
  id?: number;
}

/**
 * Logger Configuration Options
 */
export interface LoggerOptions {
  /**
   * Log level, default is 'info'
   */
  level?: LogLevel;

  /**
   * Logger name/prefix to identify the log source
   */
  name?: string;

  /**
   * Log output stream
   */
  stream?: LogStream;

  /**
   * Whether to use JSON output (when using the default stream)
   */
  json?: boolean;

  /**
   * Whether to include a timestamp (when using the default stream)
   */
  timestamp?: boolean;

  /**
   * Whether to use color output (when using the default stream)
   */
  pretty?: boolean;

  /**
   * base context
   */
  base?: LoggerContext;

  /**
   * Sampling rate (0-1), default is 1.0 (no sampling)
   * Used to reduce log volume in high-throughput scenarios
   */
  sampleRate?: number;

  /**
   * Maximum logs per second (rate limiting)
   * When exceeded, logs will be dropped or sampled
   */
  maxLogsPerSecond?: number;

  /**
   * Default category for all logs from this logger
   * Used for filtering and routing logs
   */
  category?: string;

  /**
   * Default tags for all logs from this logger
   * Used for multi-dimensional filtering
   */
  tags?: string[];
}

/**
 * Log context with category and tags support
 */
export interface LogContext extends Record<string, unknown> {
  /** Log category for filtering */
  category?: string;
  /** Log tags for multi-dimensional filtering */
  tags?: string[];
}

/**
 * Logging Interface
 * Provides unified logging operations
 */
export interface Logger {
  /**
   * Debug Level Logging
   * @param message Log message
   * @param context Context information (optional)
   */
  debug(message: string, context?: LogContext): void;

  /**
   * Message Level Logging
   * @param message Log message
   * @param context Context information (optional)
   */
  info(message: string, context?: LogContext): void;

  /**
   * Warning level log
   * @param message Log message
   * @param context Context information (optional)
   */
  warn(message: string, context?: LogContext): void;

  /**
   * Error Level Logging
   * @param message Log message
   * @param context Context information (optional)
   */
  error(message: string, context?: LogContext): void;

  /**
   * Creating a subrecorder
   * @param name Subrecorder name
   * @param additionalContext additional context information
   * @returns Subrecorder instance
   */
  child(name: string, additionalContext?: LoggerContext): Logger;

  /**
   * Setting the log level
   * @param level Log level
   */
  setLevel(level: LogLevel): void;

  /**
   * Get current log level
   * @returns the current log level
   */
  getLevel(): LogLevel;

  /**
   * Checks if the specified level is enabled
   * @param level Log level
   * @returns Enable or disable
   */
  isLevelEnabled(level: LogLevel): boolean;

  /**
   * Flush the log buffer
   * @param callback Completion callback
   */
  flush?(callback?: () => void): void;

  /**
   * Replace the log stream at runtime
   * Used to redirect logs to different outputs after initialization
   * @param stream New log stream
   */
  setStream?(stream: LogStream): void;
}

/**
 * Package Level Logger Configuration
 */
export interface PackageLoggerOptions extends LoggerOptions {
  /**
   * package name
   */
  pkg: string;
}
