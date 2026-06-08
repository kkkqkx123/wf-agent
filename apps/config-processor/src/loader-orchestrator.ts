/**
 * Loader Orchestrator
 *
 * Orchestrates the "read file → detect format → parse → merge" pipeline
 * for each config domain.  Every function here follows the same pattern:
 *
 *   1. Try each file path (stop at first success)
 *   2. Read raw content via SDK's `tryLoadConfigFile`
 *   3. Parse via SDK's `parseJson` / `parseToml`
 *   4. Merge with defaults via the corresponding SDK processor
 *   5. Return the fully resolved config
 *
 * Because file I/O is an application-layer concern, these functions live
 * in apps/config-processor, NOT in the SDK.
 */

import { tryLoadConfigFile } from "./config-file-loader.js";
import { parseJson } from "@wf-agent/sdk/api";
import { parseToml } from "@wf-agent/sdk/api";
import {
  mergeMetricsWithDefaults,
  mergeTimeoutWithDefaults,
  mergeFileCheckpointConfig,
  mergeStorageWithDefaults,
  mergeOutputWithDefaults,
  mergePresetsWithDefaults,
} from "@wf-agent/sdk/api";
import type { MetricsConfig, TimeoutConfig, FileCheckpointConfig, StorageConfig, OutputConfig, PresetsConfig } from "@wf-agent/types";

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

/**
 * Attempt to load and parse a single config file.
 * Returns the raw parsed object (unknown) or null if the file is missing.
 */
async function tryLoadRawConfig(
  filePath: string,
): Promise<Record<string, unknown> | null> {
  const loaded = await tryLoadConfigFile(filePath);
  if (loaded === null) return null;

  const { content, format } = loaded;
  const parsed: unknown =
    format === "toml" ? parseToml(content) : parseJson(content);
  return parsed as Record<string, unknown>;
}

// -----------------------------------------------------------------------
// Domain-specific loaders
// -----------------------------------------------------------------------

/**
 * Load metrics config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged MetricsConfig (with defaults applied).
 */
export async function loadMetricsConfig(
  configPaths: string[],
): Promise<MetricsConfig> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeMetricsWithDefaults(raw as Partial<MetricsConfig>);
    }
  }

  // No file found — return pure defaults
  return mergeMetricsWithDefaults({});
}

/**
 * Load timeout config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged TimeoutConfig (with defaults applied).
 */
export async function loadTimeoutConfig(
  configPaths: string[],
): Promise<Required<TimeoutConfig>> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeTimeoutWithDefaults(raw as Partial<TimeoutConfig>);
    }
  }

  return mergeTimeoutWithDefaults({});
}

/**
 * Load file-checkpoint config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @param workspaceRoot - Optional resolved workspace root (forwarded to merge).
 * @returns The merged FileCheckpointConfig (with defaults applied).
 */
export async function loadFileCheckpointConfig(
  configPaths: string[],
  workspaceRoot?: string,
): Promise<Required<FileCheckpointConfig>> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeFileCheckpointConfig(raw as Partial<FileCheckpointConfig>, workspaceRoot);
    }
  }

  return mergeFileCheckpointConfig({}, workspaceRoot);
}

/**
 * Load storage config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged StorageConfig (with defaults applied).
 */
export async function loadStorageConfig(
  configPaths: string[],
): Promise<StorageConfig> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeStorageWithDefaults(raw as Partial<StorageConfig>);
    }
  }

  return mergeStorageWithDefaults({});
}

/**
 * Load output config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged OutputConfig (with defaults applied).
 */
export async function loadOutputConfig(
  configPaths: string[],
): Promise<Required<OutputConfig>> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergeOutputWithDefaults(raw as Partial<OutputConfig>);
    }
  }

  return mergeOutputWithDefaults({});
}

/**
 * Load presets config from the first existing file in `configPaths`.
 *
 * @param configPaths - Ordered list of candidate file paths.
 * @returns The merged PresetsConfig (with defaults applied).
 */
export async function loadPresetsConfig(
  configPaths: string[],
): Promise<PresetsConfig> {
  for (const filePath of configPaths) {
    const raw = await tryLoadRawConfig(filePath);
    if (raw !== null) {
      return mergePresetsWithDefaults(raw as Partial<PresetsConfig>);
    }
  }

  return mergePresetsWithDefaults({});
}