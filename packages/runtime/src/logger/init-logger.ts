/**
 * Runtime Logger
 * Shared logger initialization for Modular Agent Framework applications.
 *
 * Provides unified initLogger / initSDKLogger functions that both
 * cli-app and server can use, eliminating duplicated logger setup.
 *
 * Usage:
 *   import { initLogger, initSDKLogger } from "@wf-agent/runtime/logger";
 *
 *   // Create a LogStream adapter for your app's output system
 *   const stream = new MyAppLogStreamAdapter();
 *   initLogger({ verbose: true }, stream, "my-app");
 *   initSDKLogger({ debug: true }, stream);
 */

import {
  createPackageLogger,
  registerLogger,
  setAllLoggersLevel,
  setupExitHandlers,
  getLogLevelFromEnv,
  configureLazyLogger,
} from "@wf-agent/common-utils";
import type { LogLevel, LogStream } from "@wf-agent/common-utils";
import { configureSDKLogger } from "@wf-agent/sdk/utils";

// ============================================
// Types
// ============================================

export interface LoggerOptions {
  verbose?: boolean;
  debug?: boolean;
  logFile?: string;
  outputDir?: string;
  logFilePattern?: string;
  enableLogTerminal?: boolean;
  enableSDKLogs?: boolean;
  sdkLogLevel?: string;
  /** Maximum log file size in bytes (default: 100MB) */
  maxLogSize?: number;
  /** Maximum number of log files to keep (default: 10) */
  maxLogFiles?: number;
}

// ============================================
// Environment Variable Names (Layered Architecture)
// ============================================

const ENV_VARS = {
  // Global settings (affect all modules)
  /** Global log level for all modules */
  GLOBAL_LOG_LEVEL: "GLOBAL_LOG_LEVEL",

  // SDK module specific
  /** SDK base log level */
  SDK_LOG_LEVEL: "SDK_LOG_LEVEL",
  /** Disable SDK logs */
  SDK_DISABLE_LOGS: "SDK_DISABLE_LOGS",
} as const;

// ============================================
// Helper Functions
// ============================================

/**
 * Determine if SDK logs should be disabled
 */
function isSDKLogsDisabled(options: LoggerOptions): boolean {
  if (process.env[ENV_VARS.SDK_DISABLE_LOGS] === "true") return true;
  if (options.enableSDKLogs === false) return true;
  return false;
}

// ============================================
// Initialization Functions
// ============================================

/**
 * Initialize application logger
 * Should be called at program startup, before SDK initialization.
 *
 * @param options Logger configuration options
 * @param stream LogStream to route log entries through
 * @param appName Application name for the logger (e.g. "cli-app", "server")
 */
export function initLogger(
  options: LoggerOptions = {},
  stream: LogStream,
  appName: string = "app",
): void {
  // Determine app log level
  const level: LogLevel = options.debug
    ? "debug"
    : options.verbose
      ? "info"
      : getLogLevelFromEnv("APP_LOG_LEVEL", ENV_VARS.GLOBAL_LOG_LEVEL, "warn");

  const logger = createPackageLogger(appName, {
    level,
    stream,
    timestamp: true,
  });

  registerLogger(appName, logger);

  // Setup process exit handlers to ensure logs are flushed
  setupExitHandlers();
}

/**
 * Initialize SDK Logger
 * Should be called before SDK initialization.
 *
 * @param options Logger configuration options
 * @param stream LogStream to route log entries through
 */
export function initSDKLogger(
  options: LoggerOptions = {},
  stream: LogStream,
): void {
  const disableSDKLogs = isSDKLogsDisabled(options);

  // Determine base SDK log level
  let baseLevel: LogLevel;
  if (disableSDKLogs) {
    baseLevel = "off";
  } else if (options.debug) {
    baseLevel = "debug";
  } else if (options.verbose) {
    baseLevel = "info";
  } else {
    baseLevel = getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL, ENV_VARS.GLOBAL_LOG_LEVEL, "off");
  }

  // Configure lazy loggers BEFORE they are initialized
  configureLazyLogger("storage", { level: baseLevel, stream });
  configureLazyLogger("tool-executors", { level: baseLevel, stream });

  // Configure the SDK logger with the determined level
  configureSDKLogger({
    level: baseLevel,
    stream,
  });

  // Set all other registered loggers to the base level
  setAllLoggersLevel(baseLevel);
}

/**
 * Initialize all loggers (app + SDK).
 * Convenience wrapper that calls initLogger() and initSDKLogger().
 */
export function initAllLoggers(
  options: LoggerOptions = {},
  stream: LogStream,
  appName: string = "app",
): void {
  initLogger(options, stream, appName);
  initSDKLogger(options, stream);
}

/**
 * Re-export environment variable names for documentation purposes
 */
export { ENV_VARS };