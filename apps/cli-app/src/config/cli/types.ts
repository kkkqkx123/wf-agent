/**
 * CLI Configuration Types
 * Contains all type definitions for CLI configuration.
 * Extends the base AppConfig from @wf-agent/runtime.
 */

import type { AppConfig } from "@wf-agent/runtime";

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
 * Extends the base AppConfig with CLI-specific fields.
 */
export interface CLIConfig extends AppConfig {
  /** Output format (table, json, plain) */
  outputFormat: OutputFormat;
  /** Maximum number of concurrent workflow executions */
  maxConcurrentExecutions: number;
  /** Storage configuration */
  storage?: StorageConfig;
  /** Output configuration */
  output?: OutputConfig;
  /** Presets configuration */
  presets?: PresetsConfig;
}
