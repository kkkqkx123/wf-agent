/**
 * MCP Connection Processor
 *
 * Pure business functions for MCP connection configuration.
 * - Zod schemas and type guards: @wf-agent/types (tool-schema.ts)
 * - Validation functions: sdk/shared/validation/mcp-validator.ts
 * - This module: merge, default creation, and bulk loading only.
 *
 * Lives here in the services layer because these are MCP-domain-specific
 * processing functions — core/ is reserved for the execution engine.
 */

import type { McpServerConfig, McpSettings, McpServerLifecycle } from "@wf-agent/types";
import { validateServerConfig } from "../../../shared/validation/mcp-validator.js";

// ---------------------------------------------------------------------------
// Processing / merging
// ---------------------------------------------------------------------------

/**
 * Load server configurations from parsed settings, validating each entry.
 * Returns both valid configs and any validation errors encountered.
 * Caller is responsible for handling errors (logging, fallback, etc.).
 */
export function loadServerConfigs(settings: McpSettings): {
  configs: Map<string, McpServerConfig>;
  errors: Array<{ name: string; error: Error }>;
} {
  const configs = new Map<string, McpServerConfig>();
  const errors: Array<{ name: string; error: Error }> = [];

  for (const [name, config] of Object.entries(settings.mcpServers || {})) {
    try {
      const validatedConfig = validateServerConfig(config, name);
      configs.set(name, validatedConfig);
    } catch (error) {
      errors.push({ name, error: error instanceof Error ? error : new Error(String(error)) });
    }
  }

  return { configs, errors };
}

/**
 * Create default empty MCP settings.
 */
export function createDefaultMcpSettings(): McpSettings {
  return { mcpServers: {} };
}

/**
 * Merge server configurations from multiple sources.
 * Project-level servers take priority over global servers.
 */
export function mergeServerConfigs(
  globalConfigs: Map<string, McpServerConfig>,
  projectConfigs: Map<string, McpServerConfig>,
): Map<string, McpServerConfig> {
  const merged = new Map<string, McpServerConfig>(globalConfigs);
  for (const [name, config] of projectConfigs) {
    merged.set(name, config);
  }
  return merged;
}

// ---------------------------------------------------------------------------
// Lifecycle helpers
// ---------------------------------------------------------------------------

/**
 * Resolved lifecycle configuration for a server.
 */
export interface ResolvedLifecycle {
  lifecycle: McpServerLifecycle;
  idleTimeout: number;
  healthCheckInterval: number;
}

/**
 * Resolve lifecycle configuration for a server, applying manager-level defaults.
 */
export function resolveServerLifecycle(
  config: McpServerConfig,
  defaults?: {
    defaultLifecycle?: McpServerLifecycle;
    defaultIdleTimeout?: number;
    defaultHealthCheckInterval?: number;
  },
): ResolvedLifecycle {
  return {
    lifecycle: config.lifecycle ?? defaults?.defaultLifecycle ?? "lazy",
    idleTimeout: config.idleTimeout ?? defaults?.defaultIdleTimeout ?? 0,
    healthCheckInterval: config.healthCheckInterval ?? defaults?.defaultHealthCheckInterval ?? 30,
  };
}
