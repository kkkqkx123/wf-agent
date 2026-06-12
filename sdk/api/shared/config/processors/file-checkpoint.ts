/**
 * File Checkpoint Configuration Processor
 *
 * Provides functions for processing and merging file checkpoint configuration.
 * This module handles the business logic for file checkpoint config without file I/O.
 *
 * Following the project architecture pattern:
 * - All configuration processing happens in api/shared/config layer
 * - Pure functions, no side effects
 * - Converts user-facing FileCheckpointConfig to runtime FileCheckpointManagerConfig
 */

import type { FileCheckpointConfig } from "@wf-agent/types";
import type { FileCheckpointManagerConfig } from "@wf-agent/common-utils";
import * as path from "node:path";

/**
 * Default file checkpoint configuration
 */
const DEFAULT_FILE_CHECKPOINT_CONFIG: Required<FileCheckpointConfig> = {
  enabled: false,
  workspaceRoot: "",
  maxDeltaChainLength: 20,
  customIgnorePatterns: [],
  storage: {
    type: "sqlite",
    dbPath: "",
  },
};

/**
 * Merge user config with defaults
 *
 * @param userConfig - User-provided partial configuration
 * @param resolvedWorkspaceRoot - Resolved workspace root (from SDK options or process.cwd())
 * @returns Merged configuration with defaults applied
 */
export function mergeFileCheckpointConfig(
  userConfig: Partial<FileCheckpointConfig>,
  resolvedWorkspaceRoot?: string,
): Required<FileCheckpointConfig> {
  const workspaceRoot = userConfig.workspaceRoot ?? resolvedWorkspaceRoot ?? "";
  const dbPath =
    userConfig.storage?.dbPath ?? path.join(workspaceRoot, "data", "file-checkpoints.db");

  return {
    enabled: userConfig.enabled ?? DEFAULT_FILE_CHECKPOINT_CONFIG.enabled,
    workspaceRoot,
    maxDeltaChainLength:
      userConfig.maxDeltaChainLength ?? DEFAULT_FILE_CHECKPOINT_CONFIG.maxDeltaChainLength,
    customIgnorePatterns:
      userConfig.customIgnorePatterns ?? DEFAULT_FILE_CHECKPOINT_CONFIG.customIgnorePatterns,
    storage: {
      type: userConfig.storage?.type ?? DEFAULT_FILE_CHECKPOINT_CONFIG.storage.type,
      dbPath,
    },
  };
}

/**
 * Convert user-facing FileCheckpointConfig to runtime FileCheckpointManagerConfig
 *
 * This bridges the declarative user config (in packages/types) with the
 * runtime manager config (in packages/storage).
 *
 * @param config - Processed file checkpoint configuration
 * @returns FileCheckpointManagerConfig suitable for FileCheckpointManager constructor
 */
export function toFileCheckpointManagerConfig(
  config: Required<FileCheckpointConfig>,
): FileCheckpointManagerConfig {
  return {
    enabled: config.enabled,
    workspaceRoot: config.workspaceRoot,
    maxDeltaChainLength: config.maxDeltaChainLength,
    customIgnorePatterns: config.customIgnorePatterns,
  };
}
