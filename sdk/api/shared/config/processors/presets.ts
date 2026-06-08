/**
 * Presets Configuration Processor
 *
 * Provides functions for processing and merging presets configuration.
 * This module handles the business logic for presets config without file I/O.
 *
 * Following the project architecture pattern:
 * - All configuration processing happens in api/shared/config layer
 * - Pure functions, no side effects
 * - No file I/O operations
 */

import type { PresetsConfig } from "@wf-agent/types";

/**
 * Default presets configuration
 */
const DEFAULT_PRESETS_CONFIG: Required<PresetsConfig> = {
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
};

/**
 * Merge user config with defaults for presets configuration.
 * Deep-merges each preset subsection independently.
 *
 * @param userConfig - User-provided partial configuration
 * @returns Merged configuration with defaults applied
 */
export function mergePresetsWithDefaults(userConfig: Partial<PresetsConfig>): PresetsConfig {
  return {
    contextCompression: userConfig.contextCompression
      ? { ...DEFAULT_PRESETS_CONFIG.contextCompression, ...userConfig.contextCompression }
      : DEFAULT_PRESETS_CONFIG.contextCompression,
    predefinedTools: userConfig.predefinedTools
      ? { ...DEFAULT_PRESETS_CONFIG.predefinedTools, ...userConfig.predefinedTools }
      : DEFAULT_PRESETS_CONFIG.predefinedTools,
    predefinedPrompts: userConfig.predefinedPrompts
      ? { ...DEFAULT_PRESETS_CONFIG.predefinedPrompts, ...userConfig.predefinedPrompts }
      : DEFAULT_PRESETS_CONFIG.predefinedPrompts,
  };
}

/**
 * Get environment-specific defaults for presets.
 */
export function getPresetsEnvironmentDefaults(env: "development" | "production"): PresetsConfig {
  if (env === "development") {
    return {
      ...DEFAULT_PRESETS_CONFIG,
      contextCompression: {
        ...DEFAULT_PRESETS_CONFIG.contextCompression,
        timeout: 15000, // Faster compression for dev feedback
      },
    };
  }

  return DEFAULT_PRESETS_CONFIG;
}