/**
 * ReadFileConfig Processor
 *
 * Validates, transforms, and exports ReadFileConfig for the read_file tool.
 * Provides runtime defaults and ensures all fields are within valid ranges.
 */

import type { ReadFileConfig } from "@wf-agent/sdk/resources";

/**
 * Input type for ReadFileConfig (all fields optional, for config file loading)
 */
export interface ReadFileConfigInput {
  workspaceDir?: string;
  maxFileSize?: number;
  maxChars?: number;
  maxLines?: number;
  enableIgnore?: boolean;
  enableProtect?: boolean;
  modelId?: string;
}

/**
 * Default values for ReadFileConfig
 */
const DEFAULTS: ReadFileConfig = {
  maxFileSize: 500000, // 500KB
  maxChars: 200000, // 200K chars
  maxLines: 2000, // 2000 lines
  enableIgnore: false,
  enableProtect: false,
};

/**
 * Validate ReadFileConfig input
 *
 * @param input - Raw configuration input
 * @returns Validation result with errors if any
 */
export function validateReadFileConfig(input: ReadFileConfigInput): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (input.maxFileSize !== undefined && input.maxFileSize < 0) {
    errors.push("maxFileSize must be a non-negative number");
  }

  if (input.maxChars !== undefined && input.maxChars < 0) {
    errors.push("maxChars must be a non-negative number");
  }

  if (input.maxLines !== undefined && input.maxLines < 1) {
    errors.push("maxLines must be at least 1");
  }

  if (input.maxFileSize !== undefined && input.maxFileSize > 100 * 1024 * 1024) {
    errors.push("maxFileSize exceeds maximum allowed value (100MB)");
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Transform raw input into a validated ReadFileConfig with defaults applied.
 *
 * @param input - Raw configuration input
 * @returns Validated ReadFileConfig
 */
export function transformReadFileConfig(input: ReadFileConfigInput = {}): ReadFileConfig {
  const validation = validateReadFileConfig(input);
  if (!validation.valid) {
    throw new Error(`Invalid ReadFileConfig: ${validation.errors.join("; ")}`);
  }

  return {
    workspaceDir: input.workspaceDir,
    maxFileSize: input.maxFileSize ?? DEFAULTS.maxFileSize,
    maxChars: input.maxChars ?? DEFAULTS.maxChars,
    maxLines: input.maxLines ?? DEFAULTS.maxLines,
    enableIgnore: input.enableIgnore ?? DEFAULTS.enableIgnore,
    enableProtect: input.enableProtect ?? DEFAULTS.enableProtect,
    modelId: input.modelId,
  };
}

/**
 * Export ReadFileConfig for serialization (e.g., to config file).
 * Strips out any runtime-only fields.
 *
 * @param config - Validated ReadFileConfig
 * @returns Serializable configuration object
 */
export function exportReadFileConfig(config: ReadFileConfig): ReadFileConfigInput {
  return {
    workspaceDir: config.workspaceDir,
    maxFileSize: config.maxFileSize,
    maxChars: config.maxChars,
    maxLines: config.maxLines,
    enableIgnore: config.enableIgnore,
    enableProtect: config.enableProtect,
    modelId: config.modelId,
  };
}
