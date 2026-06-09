/**
 * CLI Configuration Types
 * Contains all type definitions for CLI configuration.
 */

import type {
  StorageConfig,
  OutputConfig,
  LogLevel,
  OutputFormat,
} from "@wf-agent/types";

import type {
  PresetsConfig,
} from "@wf-agent/sdk/resources";

// Re-export types for convenience
export type { StorageConfig, OutputConfig, LogLevel, OutputFormat };
export type { PresetsConfig };

/**
 * Complete CLI Configuration
 */
export interface CLIConfig {
  apiUrl?: string;
  apiKey?: string;
  defaultTimeout: number;
  verbose: boolean;
  debug: boolean;
  logLevel: LogLevel;
  outputFormat: OutputFormat;
  maxConcurrentExecutions: number;
  storage?: StorageConfig;
  output?: OutputConfig;
  presets?: PresetsConfig;
}
