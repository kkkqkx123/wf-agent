/**
 * Logging Module Main Entry
 * Streaming logging system based on pino design ideas
 * Unified export of all public APIs of the logging system.
 */

// type definition
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
export { LOG_LEVEL_PRIORITY } from "./types.js";

// Core Realization
export {
  createLogger,
  createPackageLogger,
  createConsoleLogger,
  createNoopLogger,
  setGlobalLogger,
  getGlobalLogger,
  setGlobalLogLevel,
  getGlobalLogLevel,
  // Global Logging Registry API
  registerLogger,
  unregisterLogger,
  getRegisteredLogger,
  getRegisteredLoggerNames,
  setLoggerLevel,
  setAllLoggersLevel,
  // Lazy logger API
  createLazyLogger,
  configureLazyLogger,
  isLazyLoggerInitialized,
  getLazyLoggerInstance,
  type LoggerFactory,
  // Environment variable utility
  getLogLevelFromEnv,
  // Process exit handlers
  flushAllLoggers,
  flushAllLoggersSync,
  setupExitHandlers,
} from "./logger.js";

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
