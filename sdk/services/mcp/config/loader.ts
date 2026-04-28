/**
 * MCP Configuration Loader
 * Handles loading and parsing MCP server configurations
 */

import * as fs from "fs/promises";
import * as path from "path";
import type { McpSettings, McpServerConfig } from "../types.js";
import { validateMcpSettings, validateServerConfig } from "./schema.js";

/**
 * Default MCP settings file name
 */
export const DEFAULT_MCP_SETTINGS_FILE = "mcp-settings.json";

/**
 * Default project MCP file path
 */
export const PROJECT_MCP_FILE = ".agent/mcp.json";

/**
 * Load MCP settings from a file
 *
 * @param filePath - Path to the settings file
 * @returns MCP settings
 * @throws Error if file cannot be read or parsed
 */
export async function loadMcpSettings(filePath: string): Promise<McpSettings> {
  const content = await fs.readFile(filePath, "utf-8");

  let config: unknown;
  try {
    config = JSON.parse(content);
  } catch (parseError) {
    throw new Error(
      `Failed to parse MCP settings file: ${filePath}. Invalid JSON syntax.`
    );
  }

  return validateMcpSettings(config);
}

/**
 * Load server configurations from settings
 *
 * @param settings - MCP settings
 * @returns Map of server name to configuration
 */
export function loadServerConfigs(
  settings: McpSettings
): Map<string, McpServerConfig> {
  const configs = new Map<string, McpServerConfig>();

  for (const [name, config] of Object.entries(settings.mcpServers || {})) {
    try {
      const validatedConfig = validateServerConfig(config, name);
      configs.set(name, validatedConfig);
    } catch (error) {
      console.error(`Failed to validate config for server "${name}":`, error);
      // Skip invalid servers
    }
  }

  return configs;
}

/**
 * Check if a file exists
 *
 * @param filePath - Path to check
 * @returns True if file exists
 */
export async function fileExists(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

/**
 * Get global MCP settings file path
 *
 * @param settingsDir - Settings directory path
 * @returns Full path to settings file
 */
export function getGlobalMcpSettingsPath(settingsDir: string): string {
  return path.join(settingsDir, DEFAULT_MCP_SETTINGS_FILE);
}

/**
 * Get project MCP file path
 *
 * @param projectRoot - Project root directory
 * @returns Full path to project MCP file
 */
export function getProjectMcpPath(projectRoot: string): string {
  return path.join(projectRoot, PROJECT_MCP_FILE);
}

/**
 * Create default MCP settings
 *
 * @returns Default MCP settings
 */
export function createDefaultMcpSettings(): McpSettings {
  return {
    mcpServers: {},
  };
}

/**
 * Write MCP settings to file
 *
 * @param filePath - Path to write to
 * @param settings - Settings to write
 */
export async function writeMcpSettings(
  filePath: string,
  settings: McpSettings
): Promise<void> {
  const content = JSON.stringify(settings, null, 2);
  await fs.writeFile(filePath, content, "utf-8");
}

/**
 * Ensure MCP settings file exists
 *
 * @param filePath - Path to check/create
 * @returns True if file was created
 */
export async function ensureMcpSettingsFile(filePath: string): Promise<boolean> {
  if (await fileExists(filePath)) {
    return false;
  }

  // Ensure directory exists
  const dir = path.dirname(filePath);
  await fs.mkdir(dir, { recursive: true });

  // Write default settings
  await writeMcpSettings(filePath, createDefaultMcpSettings());
  return true;
}

/**
 * Merge server configurations from multiple sources
 * Project-level servers take priority over global servers
 *
 * @param globalConfigs - Global server configurations
 * @param projectConfigs - Project server configurations
 * @returns Merged configurations
 */
export function mergeServerConfigs(
  globalConfigs: Map<string, McpServerConfig>,
  projectConfigs: Map<string, McpServerConfig>
): Map<string, McpServerConfig> {
  const merged = new Map<string, McpServerConfig>();

  // Add global configs first
  for (const [name, config] of globalConfigs) {
    merged.set(name, config);
  }

  // Project configs override global configs with same name
  for (const [name, config] of projectConfigs) {
    merged.set(name, config);
  }

  return merged;
}
