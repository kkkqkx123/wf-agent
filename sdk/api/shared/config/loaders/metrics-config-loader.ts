/**
 * Metrics Configuration Loader
 *
 * Loads metrics configuration from files with priority-based resolution.
 * This is the only module in the config system that performs file I/O.
 *
 * Configuration Priority (highest to lowest):
 * 1. SDKOptions.metrics (programmatic override)
 * 2. Config file (configs/metrics.toml or metrics.json)
 * 3. Environment-specific defaults (development/production)
 * 4. Hardcoded defaults (current values as fallback)
 */

import type { MetricsConfig } from "@wf-agent/types";
import { mergeMetricsWithDefaults } from "../processors/metrics.js";
import { createConfigFileLoader } from "./config-loader-factory.js";

/**
 * Load metrics configuration from TOML or JSON file
 *
 * @param filePath - Path to configuration file
 * @returns Parsed and merged metrics configuration
 */
export const loadMetricsConfigFromFile = createConfigFileLoader<MetricsConfig>(
  mergeMetricsWithDefaults,
  "MetricsConfigLoader",
);