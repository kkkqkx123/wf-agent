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
import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "FileCheckpointConfigLoader" });

/**
 * Load file checkpoint configuration from TOML or JSON file
 *
 * @param filePath - Path to configuration file
 * @param resolvedWorkspaceRoot - Resolved workspace root path
 * @returns Merged file checkpoint configuration
 */
export async function loadFileCheckpointConfigFromFile(
  filePath: string,
  resolvedWorkspaceRoot?: string,
): Promise<Required<FileCheckpointConfig>> {
  try {
    const fs = await import("fs/promises");
    const pathModule = await import("path");

    const resolvedPath = pathModule.resolve(filePath);

    try {
      await fs.access(resolvedPath);
    } catch {
      logger.warn("File checkpoint config file not found, using defaults", {
        filePath: resolvedPath,
      });
      return mergeFileCheckpointConfig({}, resolvedWorkspaceRoot);
    }

    const content = await fs.readFile(resolvedPath, "utf-8");
    const ext = pathModule.extname(resolvedPath).toLowerCase();

    let parsed: unknown;
    if (ext === ".toml") {
      parsed = parseToml(content);
    } else if (ext === ".json") {
      parsed = parseJson(content);
    } else {
      throw new Error(`Unsupported config file format: ${ext}`);
    }

    logger.info("Loaded file checkpoint config from file", { filePath: resolvedPath });
    return mergeFileCheckpointConfig(parsed as Partial<FileCheckpointConfig>, resolvedWorkspaceRoot);
  } catch (error) {
    logger.warn("Failed to load file checkpoint config from file, using defaults", {
      filePath,
      error: error instanceof Error ? error.message : String(error),
    });
    return mergeFileCheckpointConfig({}, resolvedWorkspaceRoot);
  }
}