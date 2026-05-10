/**
 * Environment Configuration for Logger
 * Handles environment variable management and log level resolution
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - SDK_*: SDK module specific settings
 * - SDK_*_GRAPH: SDK Graph submodule settings
 * - SDK_*_AGENT: SDK Agent submodule settings
 */

import type { LogLevel } from "./types.js";

// Environment Variable Names (Layered Architecture)
export const ENV_VARS = {
  // Global settings
  GLOBAL_LOG_LEVEL: "GLOBAL_LOG_LEVEL",

  // SDK module specific
  SDK_LOG_LEVEL: "SDK_LOG_LEVEL",
} as const;

/**
 * Get log level from environment variable with fallback chain
 * Priority: primaryKey > globalKey > defaultLevel
 *
 * This is the unified utility function for reading log levels from environment variables.
 * Packages should use this instead of implementing their own logic.
 *
 * @param primaryKey - Primary environment variable key (package-specific)
 * @param globalKey - Global fallback key (defaults to GLOBAL_LOG_LEVEL)
 * @param defaultLevel - Default level if neither env var is set
 * @returns The resolved log level
 *
 * @example
 * // In a package logger:
 * const level = getLogLevelFromEnv(
 *   "SCRIPT_EXECUTORS_LOG_LEVEL",
 *   "GLOBAL_LOG_LEVEL",
 *   "info"
 * );
 */
export function getLogLevelFromEnv(
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
 * Get default log level from environment variables
 * Priority: SDK_LOG_LEVEL > GLOBAL_LOG_LEVEL > info
 */
export function getDefaultLogLevel(): LogLevel {
  return getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL, ENV_VARS.GLOBAL_LOG_LEVEL, "info");
}
