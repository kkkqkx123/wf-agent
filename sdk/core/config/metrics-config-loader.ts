/**
 * Metrics Configuration Loader
 * 
 * Loads and merges metrics configuration from various sources with priority-based resolution.
 * 
 * Configuration Priority (highest to lowest):
 * 1. SDKOptions.metrics (programmatic override)
 * 2. Config file (configs/metrics.toml or metrics.json)
 * 3. Environment-specific defaults (development/production)
 * 4. Hardcoded defaults (current values as fallback)
 */

import type { MetricsConfig, MetricCollectorConfig } from "../../api/shared/types/core-types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MetricsConfigLoader" });

/**
 * Default metrics configuration
 * Matches current hardcoded values for backward compatibility
 */
const DEFAULT_METRICS_CONFIG: MetricsConfig = {
  enabled: true,
  enablePeriodicReporting: false,
  reportingInterval: 60000,
  workflowMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  nodeMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  agentMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  eventMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  toolMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  tokenMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  templateMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  configMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  errorMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  resourceMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
  agentLoopMetrics: { bufferSize: 100, flushInterval: 5000, enablePeriodicReporting: false },
};

/**
 * Merge user config with defaults
 * Performs deep merge preserving the integrity of metrics configuration as a whole
 * 
 * @param userConfig - User-provided partial configuration
 * @returns Merged configuration with defaults applied
 */
export function mergeWithDefaults(userConfig: Partial<MetricsConfig>): MetricsConfig {
  // Start with defaults
  const merged: MetricsConfig = { ...DEFAULT_METRICS_CONFIG };
  
  // Merge global settings if provided
  if (userConfig.enabled !== undefined) merged.enabled = userConfig.enabled;
  if (userConfig.enablePeriodicReporting !== undefined) {
    merged.enablePeriodicReporting = userConfig.enablePeriodicReporting;
  }
  if (userConfig.reportingInterval !== undefined) {
    merged.reportingInterval = userConfig.reportingInterval;
  }
  
  // Merge collector configs as complete objects (maintains module cohesion)
  // This approach treats each collector config as an atomic unit
  const collectorConfigs = [
    'workflowMetrics', 'nodeMetrics', 'agentMetrics', 'eventMetrics',
    'toolMetrics', 'tokenMetrics', 'templateMetrics', 'configMetrics',
    'errorMetrics', 'resourceMetrics', 'agentLoopMetrics'
  ] as const;
  
  for (const key of collectorConfigs) {
    if (userConfig[key]) {
      // Merge at collector level: user config overrides defaults for that collector
      (merged as any)[key] = {
        ...(DEFAULT_METRICS_CONFIG[key] as MetricCollectorConfig),
        ...(userConfig[key] as MetricCollectorConfig),
      };
    }
  }
  
  return merged;
}

/**
 * Get default config for specific environment
 * Provides environment-optimized defaults
 * 
 * @param env - Environment name ("development" or "production")
 * @returns Environment-specific default configuration
 */
export function getEnvironmentDefaults(env: "development" | "production"): MetricsConfig {
  if (env === "development") {
    return {
      ...DEFAULT_METRICS_CONFIG,
      enablePeriodicReporting: true,  // Enable for debugging
      reportingInterval: 30000,       // More frequent in dev
      errorMetrics: {
        bufferSize: 10,
        flushInterval: 1000,          // Faster error reporting in dev
        enablePeriodicReporting: false,
      },
    };
  }
  
  // Production defaults - more conservative
  return {
    ...DEFAULT_METRICS_CONFIG,
    enablePeriodicReporting: false,
    reportingInterval: 60000,
    eventMetrics: {
      bufferSize: 500,                // Larger buffer for high-volume events
      flushInterval: 5000,
      enablePeriodicReporting: false,
    },
  };
}

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
      return { ...DEFAULT_METRICS_CONFIG };
    }
    
    // Read file content
    const content = await fs.readFile(resolvedPath, 'utf-8');
    
    // Determine format based on extension
    const ext = path.extname(resolvedPath).toLowerCase();
    let parsed: unknown;
    
    if (ext === '.toml') {
      // Parse TOML
      const { parseToml } = await import('../../api/shared/config/toml-parser.js');
      parsed = parseToml(content);
    } else if (ext === '.json') {
      // Parse JSON
      parsed = JSON.parse(content);
    } else {
      throw new Error(`Unsupported config file format: ${ext}`);
    }
    
    logger.info("Loaded metrics config from file", { filePath: resolvedPath });
    return mergeWithDefaults(parsed as Partial<MetricsConfig>);
  } catch (error) {
    logger.warn("Failed to load metrics config from file, using defaults", { 
      filePath, 
      error: error instanceof Error ? error.message : String(error) 
    });
    return { ...DEFAULT_METRICS_CONFIG };
  }
}
