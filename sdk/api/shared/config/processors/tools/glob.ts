/**
 * GlobConfig Processor
 *
 * Validates, transforms, and exports GlobConfig for the glob tool.
 * Provides runtime defaults and ensures all fields are within valid ranges.
 */

import type { GlobConfig } from "@wf-agent/sdk/resources";

/**
 * Input type for GlobConfig (all fields optional, for config file loading)
 */
export interface GlobConfigInput {
  workspaceDir?: string;
  maxResults?: number;
  enableIgnore?: boolean;
}

/**
 * Default values for GlobConfig
 */
const DEFAULTS: GlobConfig = {
  maxResults: 50,
  enableIgnore: true,
};

/**
 * Validate GlobConfig input
 */
export function validateGlobConfig(
  input: GlobConfigInput
): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  if (input.maxResults !== undefined && input.maxResults < 1) {
    errors.push("maxResults must be at least 1");
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Transform raw input into a validated GlobConfig with defaults applied.
 */
export function transformGlobConfig(
  input: GlobConfigInput = {}
): GlobConfig {
  const validation = validateGlobConfig(input);
  if (!validation.valid) {
    throw new Error(`Invalid GlobConfig: ${validation.errors.join("; ")}`);
  }

  return {
    workspaceDir: input.workspaceDir,
    maxResults: input.maxResults ?? DEFAULTS.maxResults,
    enableIgnore: input.enableIgnore ?? DEFAULTS.enableIgnore,
  };
}

/**
 * Export GlobConfig for serialization (e.g., to config file).
 */
export function exportGlobConfig(
  config: GlobConfig
): GlobConfigInput {
  return {
    workspaceDir: config.workspaceDir,
    maxResults: config.maxResults,
    enableIgnore: config.enableIgnore,
  };
}