/**
 * File Checkpoint Configuration Loader
 *
 * Loads file checkpoint configuration from files with priority-based resolution.
 * This is the only module in the config system that performs file I/O for file checkpoint config.
 *
 * Configuration Priority (highest to lowest):
 * 1. SDKOptions.fileCheckpoint (programmatic override)
 * 2. Config file (configs/file-checkpoint.toml or file-checkpoint.json)
 * 3. Hardcoded defaults (disabled by default)
 */

import type { FileCheckpointConfig } from "@wf-agent/types";
import { mergeFileCheckpointConfig } from "../processors/file-checkpoint.js";
import { createConfigFileLoader } from "./config-loader-factory.js";

/**
 * Load file checkpoint configuration from TOML or JSON file
 *
 * @param filePath - Path to configuration file
 * @param resolvedWorkspaceRoot - Resolved workspace root path
 * @returns Merged file checkpoint configuration
 */
export const loadFileCheckpointConfigFromFile = createConfigFileLoader<Required<FileCheckpointConfig>>(
  mergeFileCheckpointConfig,
  "FileCheckpointConfigLoader",
);