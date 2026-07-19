/**
 * Runtime Configuration Types
 * Shared configuration types for Modular Agent Framework applications.
 */

import type { StorageConfig, OutputConfig, LogLevel } from "@wf-agent/types";

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