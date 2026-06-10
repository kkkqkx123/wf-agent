/**
 * ListFilesConfig Processor
 *
 * Validates, transforms, and exports ListFilesConfig for the list_files tool.
 * Provides runtime defaults and ensures all fields are within valid ranges.
 */

import type { ListFilesConfig } from "@wf-agent/sdk/resources";

/**
 * Input type for ListFilesConfig (all fields optional, for config file loading)
 */
export interface ListFilesConfigInput {
  workspaceDir?: string;
  maxResults?: number;
  enableIgnore?: boolean;
}

/**
 * Default values for ListFilesConfig
 */
const DEFAULTS: ListFilesConfig = {
  maxResults: 1000,
  enableIgnore: true,
};

/**
 * Validate ListFilesConfig input
 */
export function validateListFilesConfig(
  input: ListFilesConfigInput
): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  if (input.maxResults !== undefined && input.maxResults < 1) {
    errors.push("maxResults must be at least 1");
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Transform raw input into a validated ListFilesConfig with defaults applied.
 */
export function transformListFilesConfig(
  input: ListFilesConfigInput = {}
): ListFilesConfig {
  const validation = validateListFilesConfig(input);
  if (!validation.valid) {
    throw new Error(`Invalid ListFilesConfig: ${validation.errors.join("; ")}`);
  }

  return {
    workspaceDir: input.workspaceDir,
    maxResults: input.maxResults ?? DEFAULTS.maxResults,
    enableIgnore: input.enableIgnore ?? DEFAULTS.enableIgnore,
  };
}

/**
 * Export ListFilesConfig for serialization (e.g., to config file).
 */
export function exportListFilesConfig(
  config: ListFilesConfig
): ListFilesConfigInput {
  return {
    workspaceDir: config.workspaceDir,
    maxResults: config.maxResults,
    enableIgnore: config.enableIgnore,
  };
}