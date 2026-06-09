/**
 * Presets Configuration Processor
 *
 * Validates, transforms, and exports PresetsConfig for predefined content.
 * Provides runtime defaults and ensures all fields are within valid ranges.
 *
 * Following the read-file.ts processor pattern:
 * - Separate input type for config file loading
 * - validate → transform → export pipeline
 * - No file I/O operations
 */

import type { PresetsConfig, PresetsConfigInput } from "@wf-agent/sdk/resources";
import { transformReadFileConfig, exportReadFileConfig, type ReadFileConfigInput } from "./tools/read-file.js";

/**
 * Default values for PresetsConfig
 */
const DEFAULTS = {
  contextCompression: {
    enabled: true,
    timeout: 30000,
    maxTriggers: 10,
  },
  predefinedTools: {
    enabled: true,
  },
  predefinedPrompts: {
    enabled: true,
  },
} as const;

/**
 * Validate PresetsConfig input
 *
 * @param input - Raw configuration input
 * @returns Validation result with errors if any
 */
export function validatePresetsConfig(
  input: PresetsConfigInput,
): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  if (input.contextCompression) {
    const cc = input.contextCompression;
    if (cc.timeout !== undefined && cc.timeout < 0) {
      errors.push("contextCompression.timeout must be a non-negative number");
    }
    if (cc.maxTriggers !== undefined && cc.maxTriggers < 1) {
      errors.push("contextCompression.maxTriggers must be at least 1");
    }
  }

  if (input.predefinedTools?.config?.runShell) {
    const rs = input.predefinedTools.config.runShell;
    if (rs.defaultTimeout !== undefined && rs.defaultTimeout < 0) {
      errors.push("predefinedTools.config.runShell.defaultTimeout must be a non-negative number");
    }
    if (rs.maxTimeout !== undefined && rs.maxTimeout < 0) {
      errors.push("predefinedTools.config.runShell.maxTimeout must be a non-negative number");
    }
    if (rs.defaultTimeout !== undefined && rs.maxTimeout !== undefined && rs.defaultTimeout > rs.maxTimeout) {
      errors.push("predefinedTools.config.runShell.defaultTimeout must not exceed maxTimeout");
    }
  }

  if (input.predefinedTools?.config?.backendShell) {
    const bs = input.predefinedTools.config.backendShell;
    if (bs.maxBackgroundTimeout !== undefined && bs.maxBackgroundTimeout < 0) {
      errors.push("predefinedTools.config.backendShell.maxBackgroundTimeout must be a non-negative number");
    }
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Transform raw input into a validated PresetsConfig with defaults applied.
 *
 * @param input - Raw configuration input
 * @returns Validated PresetsConfig
 */
export function transformPresetsConfig(
  input: PresetsConfigInput = {},
): PresetsConfig {
  const validation = validatePresetsConfig(input);
  if (!validation.valid) {
    throw new Error(`Invalid PresetsConfig: ${validation.errors.join("; ")}`);
  }

  return {
    contextCompression: input.contextCompression
      ? {
          enabled: input.contextCompression.enabled ?? DEFAULTS.contextCompression.enabled,
          prompt: input.contextCompression.prompt,
          timeout: input.contextCompression.timeout ?? DEFAULTS.contextCompression.timeout,
          maxTriggers: input.contextCompression.maxTriggers ?? DEFAULTS.contextCompression.maxTriggers,
        }
      : { ...DEFAULTS.contextCompression, prompt: undefined },
    predefinedTools: input.predefinedTools
      ? {
          enabled: input.predefinedTools.enabled ?? DEFAULTS.predefinedTools.enabled,
          allowList: input.predefinedTools.allowList,
          blockList: input.predefinedTools.blockList,
          config: input.predefinedTools.config
            ? {
                readFile: input.predefinedTools.config.readFile
                  ? transformReadFileConfig(input.predefinedTools.config.readFile as ReadFileConfigInput)
                  : undefined,
                writeFile: input.predefinedTools.config.writeFile,
                editFile: input.predefinedTools.config.editFile,
                runShell: input.predefinedTools.config.runShell,
                sessionNote: input.predefinedTools.config.sessionNote,
                backendShell: input.predefinedTools.config.backendShell,
              }
            : undefined,
        }
      : { ...DEFAULTS.predefinedTools, allowList: undefined, blockList: undefined, config: undefined },
    predefinedPrompts: input.predefinedPrompts
      ? {
          enabled: input.predefinedPrompts.enabled ?? DEFAULTS.predefinedPrompts.enabled,
        }
      : { ...DEFAULTS.predefinedPrompts },
  };
}

/**
 * Export PresetsConfig for serialization (e.g., to config file).
 * Converts resolved config back to the input shape.
 *
 * @param config - Validated PresetsConfig
 * @returns Serializable configuration object
 */
export function exportPresetsConfig(config: PresetsConfig): PresetsConfigInput {
  return {
    contextCompression: config.contextCompression
      ? {
          enabled: config.contextCompression.enabled,
          prompt: config.contextCompression.prompt,
          timeout: config.contextCompression.timeout,
          maxTriggers: config.contextCompression.maxTriggers,
        }
      : undefined,
    predefinedTools: config.predefinedTools
      ? {
          enabled: config.predefinedTools.enabled,
          allowList: config.predefinedTools.allowList,
          blockList: config.predefinedTools.blockList,
          config: config.predefinedTools.config
            ? {
                readFile: config.predefinedTools.config.readFile
                  ? exportReadFileConfig(config.predefinedTools.config.readFile)
                  : undefined,
                writeFile: config.predefinedTools.config.writeFile,
                editFile: config.predefinedTools.config.editFile,
                runShell: config.predefinedTools.config.runShell,
                sessionNote: config.predefinedTools.config.sessionNote,
                backendShell: config.predefinedTools.config.backendShell,
              }
            : undefined,
        }
      : undefined,
    predefinedPrompts: config.predefinedPrompts
      ? { enabled: config.predefinedPrompts.enabled }
      : undefined,
  };
}

/**
 * Get environment-specific defaults for presets.
 *
 * Uses `transformPresetsConfig` under the hood so defaults are always consistent.
 *
 * @param env - Environment name
 * @returns Environment-specific PresetsConfig
 */
export function getPresetsEnvironmentDefaults(env: "development" | "production"): PresetsConfig {
  const overrides: PresetsConfigInput = {};

  if (env === "development") {
    overrides.contextCompression = {
      timeout: 15000, // Faster compression for dev feedback
    };
  }

  return transformPresetsConfig(overrides);
}