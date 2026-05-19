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
import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MetricsConfigLoader" });

/**
 * Load metrics configuration from TOML or JSON file
 * 
 * @param filePath - Path to configuration file
 * @returns Parsed and merged metrics configuration
 */
export async function loadMetricsConfigFromFile(filePath: string): Promise<MetricsConfig> {
  try {
    const fs = await import('fs/promises');
    const path = await import('path');
    
    // Resolve absolute path
    const resolvedPath = path.resolve(filePath);
    
    // Check if file exists
    try {
      await fs.access(resolvedPath);
    } catch {
      logger.warn("Metrics config file not found", { filePath: resolvedPath });
      return mergeMetricsWithDefaults({});
    }
    
    // Read file content
    const content = await fs.readFile(resolvedPath, 'utf-8');
    
    // Determine format based on extension
    const ext = path.extname(resolvedPath).toLowerCase();
    let parsed: unknown;
    
    if (ext === '.toml') {
      // Parse TOML
      parsed = parseToml(content);
    } else if (ext === '.json') {
      // Parse JSON
      parsed = parseJson(content);
    } else {
      throw new Error(`Unsupported config file format: ${ext}`);
    }
    
    logger.info("Loaded metrics config from file", { filePath: resolvedPath });
    return mergeMetricsWithDefaults(parsed as Partial<MetricsConfig>);
  } catch (error) {
    logger.warn("Failed to load metrics config from file, using defaults", { 
      filePath, 
      error: error instanceof Error ? error.message : String(error) 
    });
    return mergeMetricsWithDefaults({});
  }
}
