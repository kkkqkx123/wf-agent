/**
 * MCP Connection Processor
 *
 * Pure business functions for MCP connection configuration.
 * - Zod schemas and type guards: @wf-agent/types (tool-schema.ts)
 * - Validation functions: sdk/core/validation/mcp-validator.ts
 * - This module: merge, default creation, and bulk loading only.
 */

import type { McpServerConfig, McpSettings } from "@wf-agent/types";
import { validateServerConfig } from "../../../../core/validation/mcp-validator.js";

// ---------------------------------------------------------------------------
// Processing / merging
// ---------------------------------------------------------------------------

/**
 * Load server configurations from parsed settings, validating each entry.
 * Returns both valid configs and any validation errors encountered.
 * Caller is responsible for handling errors (logging, fallback, etc.).
 */
export function loadServerConfigs(
  settings: McpSettings,
): { configs: Map<string, McpServerConfig>; errors: Array<{ name: string; error: Error }> } {
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