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
import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TimeoutConfigLoader" });

/**
 * Load timeout configuration from TOML or JSON file
 * 
 * @param filePath - Path to configuration file
 * @returns Parsed and merged timeout configuration
 */
export async function loadTimeoutConfigFromFile(filePath: string): Promise<Required<TimeoutConfig>> {
  try {
    const fs = await import('fs/promises');
    const path = await import('path');
    
    // Resolve absolute path
    const resolvedPath = path.resolve(filePath);
    
    // Check if file exists
    try {
      await fs.access(resolvedPath);
    } catch {
      logger.warn("Timeout config file not found", { filePath: resolvedPath });
      return mergeTimeoutWithDefaults({});
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
    
    logger.info("Loaded timeout config from file", { filePath: resolvedPath });
    return mergeTimeoutWithDefaults(parsed as Partial<TimeoutConfig>);
  } catch (error) {
    logger.warn("Failed to load timeout config from file, using defaults", { 
      filePath, 
      error: error instanceof Error ? error.message : String(error) 
    });
    return mergeTimeoutWithDefaults({});
  }
}
