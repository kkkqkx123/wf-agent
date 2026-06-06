/**
 * CLI Logger
 * Based on the project's common logging system, provides CLI-specific logging functionality
 * Supports separate log level configuration for graph and agent modules
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - CLI_*: CLI module specific settings
 * - SDK_*: SDK module specific settings
 * - SDK_*_GRAPH: SDK Graph submodule settings
 * - SDK_*_AGENT: SDK Agent submodule settings
 */

import {
  createPackageLogger,
  registerLogger,
  setAllLoggersLevel,
  setupExitHandlers,
  getLogLevelFromEnv,
  configureLazyLogger,
} from "@wf-agent/common-utils";
import type { LogLevel, LogStream, LogEntry } from "@wf-agent/common-utils";
import { getOutput } from "./output.js";
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

  // CLI module specific
  /** CLI module log level */
  CLI_LOG_LEVEL: "CLI_LOG_LEVEL",

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
// LogStream Adapter
// ============================================

/**
 * Adapter that routes LogStream writes through CLIOutput's single log stream.
 * This eliminates dual-file-write conflicts between the logger system and CLIOutput.
 */
class OutputLogStreamAdapter implements LogStream {
  write(entry: LogEntry): void {
    const output = getOutput();
    const context = entry.context as Record<string, unknown> | undefined;
    if (entry.timestamp) {
      output.log(entry.level, entry.message, {
        ...context,
        _ts: entry.timestamp,
      });
    } else {
      output.log(entry.level, entry.message, context);
    }
  }
}

const outputLogStream: LogStream = new OutputLogStreamAdapter();

// ============================================
// Initialization Functions
// ============================================

/**
 * Initialize CLI Logger
 * Should be called at program startup, before SDK initialization
 */
export function initLogger(options: LoggerOptions = {}): void {
  // Determine CLI log level
  const level: LogLevel = options.debug
    ? "debug"
    : options.verbose
      ? "info"
      : getLogLevelFromEnv(ENV_VARS.CLI_LOG_LEVEL, ENV_VARS.GLOBAL_LOG_LEVEL, "warn");

  // Use the OutputLogStreamAdapter to route through CLIOutput's single log stream.
  // This avoids creating a separate rotating file stream that would compete with CLIOutput.
  const logger = createPackageLogger("cli-app", {
    level,
    stream: outputLogStream,
    timestamp: true,
  });

  registerLogger("cli-app", logger);

  // Setup process exit handlers to ensure logs are flushed
  setupExitHandlers();
}

/**
 * Initialize SDK Logger
 * Should be called before SDK initialization
 */
export function initSDKLogger(options: LoggerOptions = {}): void {
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

  // Use the OutputLogStreamAdapter for SDK logs as well, routing through CLIOutput's
  // single log stream instead of creating a separate rotating file stream.

  // Configure lazy loggers BEFORE they are initialized
  // This ensures they use the correct configuration when first accessed
  configureLazyLogger("storage", { level: baseLevel, stream: outputLogStream });
  configureLazyLogger("tool-executors", { level: baseLevel, stream: outputLogStream });

  // Configure the SDK logger with the determined level
  configureSDKLogger({
    level: baseLevel,
    stream: outputLogStream,
  });

  // Set all other registered loggers to the base level
  // Note: This only affects already-registered loggers (e.g., cli-app)
  // Lazy loggers that haven't been initialized yet will use their pending config
  setAllLoggersLevel(baseLevel);
}

/**
 * Initialize all Loggers (CLI + SDK)
 *
 * IMPORTANT: Call this function BEFORE importing or using any SDK modules
 * to ensure proper logger configuration.
 *
 * Correct order:
 * 1. initAllLoggers() or initLogger() + initSDKLogger()
 * 2. Import SDK modules
 * 3. Use SDK functionality
 *
 * Wrong order (may cause configuration issues):
 * 1. Import SDK modules (triggers lazy logger access)
 * 2. initAllLoggers() (configuration applied too late)
 *
 * @param options Logger configuration options
 */
export function initAllLoggers(options: LoggerOptions = {}): void {
  initLogger(options);
  initSDKLogger(options);
}

/**
 * Re-export environment variable names for documentation purposes
 */
export { ENV_VARS };
