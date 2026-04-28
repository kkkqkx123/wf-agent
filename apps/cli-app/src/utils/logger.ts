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
  createRotatingFileStream,
  setupExitHandlers,
} from "@wf-agent/common-utils";
import type { LogLevel } from "@wf-agent/common-utils";
import { getOutput } from "./output.js";
import { configureSDKLogger } from "@wf-agent/sdk";

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
  /** Specific log level for graph module */
  graphLogLevel?: string;
  /** Specific log level for agent module */
  agentLogLevel?: string;
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
  /** Global max log file size */
  GLOBAL_LOG_MAX_SIZE: "GLOBAL_LOG_MAX_SIZE",
  /** Global max log files to keep */
  GLOBAL_LOG_MAX_FILES: "GLOBAL_LOG_MAX_FILES",

  // CLI module specific
  /** CLI module log level */
  CLI_LOG_LEVEL: "CLI_LOG_LEVEL",

  // SDK module specific
  /** SDK base log level */
  SDK_LOG_LEVEL: "SDK_LOG_LEVEL",
  /** SDK Graph submodule log level */
  SDK_LOG_LEVEL_GRAPH: "SDK_LOG_LEVEL_GRAPH",
  /** SDK Agent submodule log level */
  SDK_LOG_LEVEL_AGENT: "SDK_LOG_LEVEL_AGENT",
  /** Disable SDK logs */
  SDK_DISABLE_LOGS: "SDK_DISABLE_LOGS",
} as const;

// ============================================
// Helper Functions
// ============================================

/**
 * Get log level from environment variable with fallback chain
 * Priority: primaryKey > globalKey > defaultLevel
 */
function getLogLevelFromEnv(
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
 * Get numeric value from environment variable
 */
function getNumericEnv(primaryKey: string, defaultValue: number): number {
  const value = process.env[primaryKey];
  if (value) {
    const parsed = parseInt(value, 10);
    if (!isNaN(parsed)) return parsed;
  }
  return defaultValue;
}

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
 * Initialize CLI Logger
 * Should be called at program startup, before SDK initialization
 */
export function initLogger(options: LoggerOptions = {}): void {
  const output = getOutput();

  // Determine CLI log level
  const level: LogLevel = options.debug
    ? "debug"
    : options.verbose
      ? "info"
      : getLogLevelFromEnv(ENV_VARS.CLI_LOG_LEVEL, ENV_VARS.GLOBAL_LOG_LEVEL, "warn");

  // Get log rotation configuration
  const maxLogSize = options.maxLogSize ?? getNumericEnv(ENV_VARS.GLOBAL_LOG_MAX_SIZE, 104857600); // 100MB default

  const maxLogFiles = options.maxLogFiles ?? getNumericEnv(ENV_VARS.GLOBAL_LOG_MAX_FILES, 10);

  // Create a rotating file stream for CLI logs
  const fileStream = createRotatingFileStream({
    filePath: output.logFile,
    maxSize: maxLogSize,
    maxFiles: maxLogFiles,
    json: true,
    timestamp: true,
  });

  const logger = createPackageLogger("cli-app", {
    level,
    stream: fileStream,
    timestamp: true,
  });

  registerLogger("cli-app", logger);

  // Setup process exit handlers to ensure logs are flushed
  setupExitHandlers();
}

/**
 * Initialize SDK Logger
 * Supports separate log levels for graph and agent modules
 * Should be called before SDK initialization
 */
export function initSDKLogger(options: LoggerOptions = {}): void {
  const output = getOutput();

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

  // Determine module-specific log levels
  const graphLevel = options.graphLogLevel
    ? (options.graphLogLevel as LogLevel)
    : getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL_GRAPH, ENV_VARS.GLOBAL_LOG_LEVEL, baseLevel);

  const agentLevel = options.agentLogLevel
    ? (options.agentLogLevel as LogLevel)
    : getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL_AGENT, ENV_VARS.GLOBAL_LOG_LEVEL, baseLevel);

  // Get log rotation configuration
  const maxLogSize = options.maxLogSize ?? getNumericEnv(ENV_VARS.GLOBAL_LOG_MAX_SIZE, 104857600);

  const maxLogFiles = options.maxLogFiles ?? getNumericEnv(ENV_VARS.GLOBAL_LOG_MAX_FILES, 10);

  // Create a rotating file stream for SDK logs
  const fileStream = createRotatingFileStream({
    filePath: output.logFile,
    maxSize: maxLogSize,
    maxFiles: maxLogFiles,
    json: true,
    timestamp: true,
  });

  // Configure the SDK logger with separate levels for graph and agent
  configureSDKLogger({
    level: baseLevel,
    graphLevel,
    agentLevel,
    stream: fileStream,
  });

  // Also set all other registered loggers to the base level
  setAllLoggersLevel(baseLevel);
}

/**
 * Initialize all Loggers (CLI + SDK)
 */
export function initAllLoggers(options: LoggerOptions = {}): void {
  initLogger(options);
  initSDKLogger(options);
}

/**
 * Re-export environment variable names for documentation purposes
 */
export { ENV_VARS };
