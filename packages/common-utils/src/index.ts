/**
 * General Toolkit
 *
 * Exports general functionalities such as utility functions, and a logging system.
 */

// Utility functions
export * from "./utils/index.js";

// Error Handling Tools
export * from "./error/index.js";

// Code Security Tool Module
export * from "./script-security/index.js";

// Dependency Injection Container
export * from "./di/index.js";

// Process management utilities (cross-platform signal handling)
export * from "./utils/process/index.js";

// Compression utilities
export * from "./utils/compression/index.js";

// Codec (serialization/deserialization)
export * from "./codec/index.js";

// File Monitoring Module (file watching, checkpoint management)
export * from "./file-monitoring/index.js";

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
  checkLoggerInitialization,
  // Environment variable utility
  getLogLevelFromEnv,
  // Process exit handlers
  flushAllLoggers,
  flushAllLoggersSync,
  setupExitHandlers,
  // Logger cleanup utilities (for testing)
  clearLazyLoggerCache,
  clearLoggerRegistry,
  resetGlobalLogger,
  resetLoggerSystem,
  getLoggerSystemStats,
} from "./logger/index.js";

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
