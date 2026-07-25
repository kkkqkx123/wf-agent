/**
 * CLI Configuration Types
 * Contains all type definitions for CLI configuration.
 * Uses DefaultAppConfig from @wf-agent/runtime as base.
 */

import type { DefaultAppConfig } from "@wf-agent/runtime";

/**
 * Complete CLI Configuration
 */
export type CLIConfig = DefaultAppConfig;

// Re-export types for convenience
export type { StorageConfig, OutputConfig, LogLevel, OutputFormat } from "@wf-agent/types";
export type { PresetsConfig } from "@wf-agent/sdk/resources";
