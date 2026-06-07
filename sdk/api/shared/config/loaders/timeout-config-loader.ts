/**
 * Timeout Configuration Loader
 *
 * Loads timeout configuration from files with priority-based resolution.
 * This is the only module in the config system that performs file I/O for timeout config.
 *
 * Configuration Priority (highest to lowest):
 * 1. SDKOptions.timeout (programmatic override)
 * 2. Config file (configs/timeout.toml or timeout.json)
 * 3. Environment-specific defaults (development/production)
 * 4. Hardcoded defaults (current values as fallback)
 */

import type { TimeoutConfig } from "@wf-agent/types";
import { mergeTimeoutWithDefaults } from "../processors/timeout.js";
import { createConfigFileLoader } from "./config-loader-factory.js";

/**
 * Load timeout configuration from TOML or JSON file
 *
 * @param filePath - Path to configuration file
 * @returns Parsed and merged timeout configuration
 */
export const loadTimeoutConfigFromFile = createConfigFileLoader<Required<TimeoutConfig>>(
  mergeTimeoutWithDefaults,
  "TimeoutConfigLoader",
);