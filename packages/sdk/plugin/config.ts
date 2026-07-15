/**
 * Plugin Configuration - Configuration utilities and defaults for the plugin system.
 */

import type { PluginSystemOptions } from "./types.js";

// ============================================================
// Override Policy
// ============================================================

export enum OverridePolicy {
  /** Plugin cannot override existing contributions (default) */
  FORBID = 'forbid',
  /** Plugin override with warning */
  WARN = 'warn',
  /** Plugin override silently, last wins */
  ALLOW = 'allow',
  /** Plugin override by priority */
  PRIORITY = 'priority',
}

// ============================================================
// Defaults
// ============================================================

export const DEFAULT_PLUGIN_OPTIONS: PluginSystemOptions = {
  enabled: false,
  paths: ['./plugins'],
  autoActivate: true,
  guardTimeout: 10000,
  overridePolicy: OverridePolicy.FORBID,
  allowList: [],
  blockList: [],
  config: {},
};

// ============================================================
// Merge Utilities
// ============================================================

/**
 * Merge user-provided plugin options with defaults.
 */
export function mergePluginOptions(
  userOptions?: Partial<PluginSystemOptions>,
): PluginSystemOptions {
  if (!userOptions) {
    return { ...DEFAULT_PLUGIN_OPTIONS };
  }

  return {
    ...DEFAULT_PLUGIN_OPTIONS,
    ...userOptions,
    paths: userOptions.paths || DEFAULT_PLUGIN_OPTIONS.paths,
    config: userOptions.config || {},
  };
}