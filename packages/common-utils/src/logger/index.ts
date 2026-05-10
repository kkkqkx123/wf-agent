/**
 * Logging Module Main Entry
 * Streaming logging system based on pino design ideas
 * Unified export of all public APIs of the logging system.
 */

// Type definitions
export type {
  Logger,
  LogLevel,
  LoggerContext,
  LoggerOptions,
  PackageLoggerOptions,
  LogStream,
  LogEntry,
  LogContext,
  StreamOptions,
  MultistreamOptions,
  StreamEntry,
} from "./types.js";
export { LOG_LEVEL_PRIORITY, LOG_SCHEMA_VERSION } from "./types.js";

// Environment Configuration
export {
  ENV_VARS,
  getLogLevelFromEnv,
  getDefaultLogLevel,
} from "./env-config.js";

// Core Logger Classes
export { BaseLogger } from "./base-logger.js";

// Rate Limiter
export { LogRateLimiter } from "./rate-limiter.js";

// Lazy Logger System
export {
  createLazyLogger,
  configureLazyLogger,
  isLazyLoggerInitialized,
  getLazyLoggerInstance,
  checkLoggerInitialization,
  type LoggerFactory,
} from "./lazy-logger.js";

// Factory Functions
export {
  createLogger,
  createPackageLogger,
  createConsoleLogger,
  createNoopLogger,
} from "./logger-factory.js";

// Global Logger Management
export {
  setGlobalLogger,
  getGlobalLogger,
  setGlobalLogLevel,
  getGlobalLogLevel,
} from "./global-logger.js";

// Logger Registry & Process Management
export {
  loggerRegistry,
  registerLogger,
  unregisterLogger,
  getRegisteredLogger,
  getRegisteredLoggerNames,
  setLoggerLevel,
  setAllLoggersLevel,
  flushAllLoggersSync,
  flushAllLoggers,
  setupExitHandlers,
} from "./logger-registry.js";

// Logger Cleanup Utilities (for testing)
export {
  clearLazyLoggerCache,
  clearLoggerRegistry,
  resetGlobalLogger,
  resetLoggerSystem,
  getLoggerSystemStats,
} from "./logger-cleanup.js";

// Stream Implementation
export {
  BaseFileStream,
  type BaseFileStreamOptions,
  ConsoleStream,
  createConsoleStream,
  FileStream,
  createFileStream,
  type FileStreamOptions,
  RotatingFileStream,
  createRotatingFileStream,
  type RotatingFileStreamOptions,
  AsyncStream,
  createAsyncStream,
  Multistream,
  createMultistream,
} from "./streams/index.js";

// Transport implementation
export { destination, transport } from "./transports/index.js";
export type { Destination, TransportOptions, MultiTransportOptions } from "./transports/index.js";

// instrumented function
export { shouldLog, formatTimestamp, mergeContext, createLogEntry } from "./utils.js";

// Async context for distributed tracing
export {
  runWithContext,
  runWithTrace,
  getContextValue,
  getTraceId,
  getSpanId,
  setContextValue,
  createChildSpan,
  hasContext,
  generateTraceId,
  generateSpanId,
  TRACE_ID_KEY,
  SPAN_ID_KEY,
  PARENT_SPAN_ID_KEY,
} from "./async-context.js";
