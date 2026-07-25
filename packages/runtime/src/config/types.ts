/**
 * Runtime Configuration Types
 * Shared configuration types for Modular Agent Framework applications.
 */

import type { StorageConfig, OutputConfig, LogLevel, OutputFormat, PresetsConfig } from "@wf-agent/types";
import type { ExecutionMode } from "../mode/types.js";

/**
 * Runtime configuration with storage settings
 */
export interface RuntimeStorageConfig {
  /** Storage configuration */
  storage?: StorageConfig;
  /** Application name for default db path */
  appName?: string;
}

/**
 * Base application configuration.
 * Shared between cli-app, server, and other apps.
 * Each app extends this with its own specific fields.
 */
export interface AppConfig {
  /** Default timeout for operations (ms) */
  defaultTimeout: number;
  /** Enable verbose mode */
  verbose: boolean;
  /** Enable debug mode */
  debug: boolean;
  /** Log level */
  logLevel: LogLevel;
  /** Storage configuration */
  storage?: StorageConfig;
  /** Output configuration */
  output?: OutputConfig;
}

/**
 * Default application configuration.
 * Extended by cli-app and server with the common outputFormat and maxConcurrentExecutions
 * that both applications share.
 *
 * Using a type alias (intersection) instead of interface to ensure structural
 * compatibility with Record<string, unknown> in createAppConfigLoader's generic constraint.
 */
export type DefaultAppConfig = AppConfig & {
  /** Output format (table, json, plain) */
  outputFormat: OutputFormat;
  /** Maximum number of concurrent workflow executions */
  maxConcurrentExecutions: number;
  /** Presets configuration */
  presets?: PresetsConfig;
  /** Default application runtime mode (interactive/headless/programmatic).
   *  When set, this serves as the fallback if no environment variable overrides it.
   *  If not set, defaults to "interactive". */
  executionMode?: ExecutionMode;
};
