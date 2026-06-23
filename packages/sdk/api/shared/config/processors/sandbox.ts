/**
 * Sandbox Configuration Processor
 *
 * Provides functions for processing and merging sandbox configuration.
 * This module handles the business logic for sandbox global config without file I/O.
 *
 * Following the project architecture pattern:
 * - All configuration processing happens in a../shared/core/config layer
 * - Pure functions, no side effects
 * - No file I/O operations
 */

import type { SandboxGlobalConfig, SandboxProfile } from "@wf-agent/types";

/**
 * Default sandbox global configuration
 * Sandbox is disabled by default; no profiles or rules are predefined.
 */
const DEFAULT_SANDBOX_GLOBAL_CONFIG: SandboxGlobalConfig = {
  mode: "disabled",
};

/**
 * Merge user config with defaults.
 * Performs shallow merge at the top level and deep merge for profiles array.
 *
 * @param userConfig - User-provided partial configuration
 * @returns Merged configuration with defaults applied
 */
export function mergeSandboxWithDefaults(
  userConfig: Partial<SandboxGlobalConfig>,
): SandboxGlobalConfig {
  return {
    mode: userConfig.mode ?? DEFAULT_SANDBOX_GLOBAL_CONFIG.mode,
    profiles: userConfig.profiles ?? DEFAULT_SANDBOX_GLOBAL_CONFIG.profiles,
    rules: userConfig.rules ?? DEFAULT_SANDBOX_GLOBAL_CONFIG.rules,
    defaultProfile: userConfig.defaultProfile ?? DEFAULT_SANDBOX_GLOBAL_CONFIG.defaultProfile,
  };
}

/**
 * Validate sandbox global configuration.
 * Checks for basic structural integrity:
 *   - If rules are defined, all referenced profiles must exist
 *   - If defaultProfile is set, it must exist in profiles
 *
 * @param config - The sandbox config to validate
 * @returns Validation result with errors if any
 */
export function validateSandboxConfig(config: SandboxGlobalConfig): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  const profileNames = new Set((config.profiles ?? []).map((p: SandboxProfile) => p.name));

  // Validate rules reference existing profiles
  if (config.rules) {
    for (const rule of config.rules) {
      if (!profileNames.has(rule.profile)) {
        errors.push(`Sandbox rule references profile "${rule.profile}" which is not defined`);
      }
    }
  }

  // Validate defaultProfile exists
  if (config.defaultProfile && !profileNames.has(config.defaultProfile)) {
    errors.push(`Sandbox defaultProfile "${config.defaultProfile}" is not defined in profiles`);
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Transform sandbox configuration.
 * Currently a pass-through; may apply parameter substitution or other
 * transformations in the future.
 *
 * @param config - The validated sandbox config
 * @returns The transformed SandboxGlobalConfig
 */
export function transformSandboxConfig(config: SandboxGlobalConfig): SandboxGlobalConfig {
  return { ...config };
}

/**
 * Export sandbox configuration for serialization (e.g., to config file).
 * Converts resolved config back to a minimal serializable shape.
 *
 * @param config - Validated SandboxGlobalConfig
 * @returns Serializable configuration object
 */
export function exportSandboxConfig(config: SandboxGlobalConfig): Partial<SandboxGlobalConfig> {
  return {
    mode: config.mode !== DEFAULT_SANDBOX_GLOBAL_CONFIG.mode ? config.mode : undefined,
    profiles: config.profiles && config.profiles.length > 0 ? config.profiles : undefined,
    rules: config.rules && config.rules.length > 0 ? config.rules : undefined,
    defaultProfile:
      config.defaultProfile !== DEFAULT_SANDBOX_GLOBAL_CONFIG.defaultProfile
        ? config.defaultProfile
        : undefined,
  };
}
