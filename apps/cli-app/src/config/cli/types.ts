/**
 * CLI Configuration Types
 * Contains all type definitions for CLI configuration.
 */

import type {
  StorageConfig,
  OutputConfig,
  PresetsConfig,
  LogLevel,
  OutputFormat,
  RoutingRule,
} from "@wf-agent/types";

// Re-export types from @wf-agent/types for convenience
export type { StorageConfig, OutputConfig, PresetsConfig, LogLevel, OutputFormat, RoutingRule };

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
  customRoutingRules?: RoutingRule[];
}
