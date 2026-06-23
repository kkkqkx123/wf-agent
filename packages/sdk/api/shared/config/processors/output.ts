/**
 * Output Configuration Processor
 *
 * Provides functions for processing and merging output/logging configuration.
 * This module handles the business logic for output config without file I/O.
 *
 * Following the project architecture pattern:
 * - All configuration processing happens in a../shared/core/config layer
 * - Pure functions, no side effects
 * - No file I/O operations
 */

import type { OutputConfig, SDKLogLevel } from "@wf-agent/types";

/**
 * Default output configuration
 */
const DEFAULT_OUTPUT_CONFIG: Required<OutputConfig> = {
  dir: "./outputs",
  logFilePattern: "app-{date}.log",
  enableLogTerminal: true,
  enableSDKLogs: true,
  sdkLogLevel: "warn",
};

/**
 * Merge user config with defaults for output configuration.
 *
 * @param userConfig - User-provided partial configuration
 * @returns Merged configuration with defaults applied
 */
export function mergeOutputWithDefaults(userConfig: Partial<OutputConfig>): Required<OutputConfig> {
  return {
    ...DEFAULT_OUTPUT_CONFIG,
    ...userConfig,
  };
}

/**
 * Get environment-specific defaults for output configuration.
 */
export function getOutputEnvironmentDefaults(
  env: "development" | "production",
): Required<OutputConfig> {
  if (env === "development") {
    return {
      ...DEFAULT_OUTPUT_CONFIG,
      sdkLogLevel: "debug" as SDKLogLevel,
      enableLogTerminal: true,
    };
  }

  // Production: less verbose, file-only
  return {
    ...DEFAULT_OUTPUT_CONFIG,
    sdkLogLevel: "error" as SDKLogLevel,
    enableLogTerminal: false,
  };
}
