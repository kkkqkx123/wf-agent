/**
 * General Toolkit
 *
 * Exports general functionalities such as HTTP transfer, utility functions, and a logging system.
 */

// HTTP-related
export * from "./http/index.js";

// Utility functions
export * from "./utils/index.js";

// Template renderer
export * from "./template/template-renderer.js";

// Error Handling Tools
export * from "./error/index.js";

// Expression evaluator
export * from "./evalutor/index.js";

// Tool Definition Translation Module - Moved to sdk/core/llm/utils/

// Message Management Module
export * from "./message/index.js";

// Code Security Tool Module
export * from "./script-security/index.js";

// Dependency Injection Container
export * from "./di/index.js";

// AbortSignal and thread interruption tools
export * from "./utils/signal/abort-utils.js";
export * from "./utils/signal/thread-interruption-utils.js";
export * from "./utils/signal/interruption-types.js";

// Log system (selective export to avoid naming conflicts)
export {
  createLogger,
  createPackageLogger,
  createConsoleLogger,
  createNoopLogger,
  setGlobalLogger,
  getGlobalLogger,
  setGlobalLogLevel,
  getGlobalLogLevel,
  // Global Log Registry API
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
  // Environment variable utility
  getLogLevelFromEnv,
  // Process exit handlers
  flushAllLoggers,
  flushAllLoggersSync,
  setupExitHandlers,
} from "./logger/logger.js";

export {
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
} from "./logger/streams/index.js";

export { destination, transport } from "./logger/transports/index.js";

export { shouldLog, formatTimestamp, mergeContext, createLogEntry } from "./logger/utils.js";

export type {
  Logger,
  LogLevel,
  LoggerContext,
  LoggerOptions,
  PackageLoggerOptions,
  LogStream,
  LogEntry,
  StreamOptions,
  MultistreamOptions,
  StreamEntry,
} from "./logger/types.js";

export { LOG_LEVEL_PRIORITY } from "./logger/types.js";

export type {
  Destination,
  TransportOptions,
  MultiTransportOptions,
} from "./logger/transports/index.js";
