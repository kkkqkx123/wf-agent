/**
 * Runtime Config Defaults
 * Default configuration values shared across applications.
 * Each app can extend this with its own defaults.
 */

import type { AppConfig } from "./types.js";

/**
 * Default configuration values.
 * Apps should merge their specific defaults over this base.
 */
export const DEFAULT_CONFIG: AppConfig = {
  defaultTimeout: 30000,
  verbose: false,
  debug: false,
  logLevel: "warn",
};