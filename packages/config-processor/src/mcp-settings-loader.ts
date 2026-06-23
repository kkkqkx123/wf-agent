/**
 * MCP Settings Loader
 *
 * File I/O operations for MCP server configuration settings.
 * Lives in the application layer — the SDK only provides pure processing
 * functions (createDefaultMcpSettings, loadServerConfigs, mergeServerConfigs).
 */

import * as fs from "fs/promises";
import * as path from "path";
import { fileExists } from "@wf-agent/common-utils";
import type { McpSettings } from "@wf-agent/types";
import { McpSettingsSchema } from "@wf-agent/types";
import { createDefaultMcpSettings, loadServerConfigs, mergeServerConfigs } from "@wf-agent/sdk/services";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/**
 * Default MCP settings file name (global)
 */
export const DEFAULT_MCP_SETTINGS_FILE = "mcp-settings.json";

/**
 * Default project MCP file path (.agent/, traditional convention)
 */
export const PROJECT_MCP_FILE = ".agent/mcp.json";

/**
 * Project MCP file path (.wf/, higher priority)
 */
export const PROJECT_WF_MCP_FILE = ".wf/mcp.json";

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/**
 * Get global MCP settings file path
 */
export function getGlobalMcpSettingsPath(settingsDir: string): string {
  return path.join(settingsDir, DEFAULT_MCP_SETTINGS_FILE);
}

/**
 * Get project MCP file path (.agent/)
 */
export function getProjectMcpPath(projectRoot: string): string {
  return path.join(projectRoot, PROJECT_MCP_FILE);
}

/**
 * Get project MCP file path (.wf/, higher priority)
 */
export function getProjectWfMcpPath(projectRoot: string): string {
  return path.join(projectRoot, PROJECT_WF_MCP_FILE);
}

/**
 * Get all possible project MCP file paths in priority order.
 * .wf/ takes priority over .agent/.
 */
export function getProjectMcpPaths(projectRoot: string): string[] {
  return [
    getProjectWfMcpPath(projectRoot),
    getProjectMcpPath(projectRoot),
  ];
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/**
 * Load MCP settings from a JSON file.
 */
export async function loadMcpSettings(filePath: string): Promise<McpSettings> {
  const content = await fs.readFile(filePath, "utf-8");

  let config: unknown;
  try {
    config = JSON.parse(content);
  } catch (parseError) {
    throw new Error(
      `Failed to parse MCP settings file: ${filePath}. Invalid JSON syntax.`,
      { cause: parseError },
    );
  }

  const result = McpSettingsSchema.safeParse(config);
  if (!result.success) {
    const errorMessages = result.error.issues
      .map((err) => `${err.path.join(".")}: ${err.message}`)
      .join("\n");
    throw new Error(`Invalid MCP settings: ${errorMessages}`);
  }

  return result.data;
}

/**
 * Write MCP settings to a JSON file.
 */
export async function writeMcpSettings(
  filePath: string,
  settings: McpSettings,
): Promise<void> {
  const content = JSON.stringify(settings, null, 2);
  await fs.writeFile(filePath, content, "utf-8");
}

/**
 * Ensure MCP settings file exists, creating a default one if absent.
 * @returns true if the file was created, false if it already existed.
 */
export async function ensureMcpSettingsFile(filePath: string): Promise<boolean> {
  if (await fileExists(filePath)) {
    return false;
  }

  const dir = path.dirname(filePath);
  await fs.mkdir(dir, { recursive: true });
  await writeMcpSettings(filePath, createDefaultMcpSettings());
  return true;
}

// ---------------------------------------------------------------------------
// Composite loader
// ---------------------------------------------------------------------------

/**
 * Load and merge MCP settings from global and all project-level config files.
 *
 * Priority chain (highest first):
 *   1. .wf/mcp.json (project-specific, highest priority)
 *   2. .agent/mcp.json (project-specific)
 *   3. Global config at settingsDir (lowest priority)
 *
 * Server configs with the same key are overridden by higher-priority sources.
 *
 * @param settingsDir - Global settings directory
 * @param projectRoot - Absolute path to the project root
 * @returns Merged McpSettings
 */
export async function loadAndMergeMcpSettings(
  settingsDir: string,
  projectRoot: string,
): Promise<McpSettings> {
  const globalPath = getGlobalMcpSettingsPath(settingsDir);
  const projectPaths = getProjectMcpPaths(projectRoot);

  // Load from all sources (null if file doesn't exist)
  const [globalSettings, ...projectSettings] = await Promise.all([
    loadMcpSettings(globalPath).catch(() => null),
    ...projectPaths.map((p) => loadMcpSettings(p).catch(() => null)),
  ]);

  // Start with global as the base
  const baseSettings = globalSettings ?? createDefaultMcpSettings();
  const globalConfigs = loadServerConfigs(baseSettings).configs;

  // Merge project layers in priority order (last wins in the chain)
  // projectPaths[0] = .wf/mcp.json (highest), projectPaths[1] = .agent/mcp.json
  let mergedConfigs = new Map(globalConfigs);

  for (const settings of projectSettings) {
    if (settings !== null) {
      const projectConfigs = loadServerConfigs(settings).configs;
      mergedConfigs = mergeServerConfigs(mergedConfigs, projectConfigs);
    }
  }

  return {
    mcpServers: Object.fromEntries(mergedConfigs),
  };
}

// ---------------------------------------------------------------------------
// Preset-based MCP Loading
// ---------------------------------------------------------------------------

/**
 * Default MCP preset directory (configs/mcp in project root).
 */
export function getDefaultMcpPresetDir(projectRoot: string): string {
  return path.join(projectRoot, "configs", "mcp");
}

/**
 * Load MCP settings from a preset by name.
 *
 * Preset resolution:
 * 1. Load `configs/mcp/index.json` → expand paths → discover presets
 * 2. Match `presetName` to a preset file by filename
 * 3. Load and validate the preset file content as McpSettings
 *
 * @param baseDir - Directory containing the preset index (e.g. `configs/mcp`)
 * @param presetName - Name of the preset to load (matches filename without extension)
 * @returns Parsed McpSettings
 * @throws Error if preset index is missing or preset name is not found
 */
export async function loadMcpPresetSettings(
  baseDir: string,
  presetName: string,
): Promise<McpSettings> {
  const { resolvePresetIndex, findPresetByName, loadSingleFilePreset } = await import("./preset-loader.js");
  const resolved = await resolvePresetIndex(baseDir);
  const entry = findPresetByName(resolved.presets, presetName);

  if (!entry) {
    const available = Array.from(resolved.presets.keys()).join(", ");
    throw new Error(
      `MCP preset "${presetName}" not found in ${baseDir}. Available presets: ${available || "(none)"}`,
    );
  }

  const settings = await loadSingleFilePreset<McpSettings>(entry);
  return settings;
}

/**
 * Load MCP settings with preset support.
 *
 * Tries preset mode first (if `configs/mcp/index.json` exists), then
 * falls back to the legacy global/project config chain.
 *
 * Preset loading flow:
 * 1. Load `configs/mcp/index.json` → scan presets → match by name
 * 2. Load the matched preset file for the base config
 * 3. Merge with instance-level overrides (`.wf/mcp.json`, `.agent/mcp.json`, global)
 *
 * @param settingsDir - Global settings directory
 * @param projectRoot - Absolute path to the project root
 * @param presetName - Name of the preset to use (optional)
 * @returns Merged McpSettings
 */
export async function loadAndMergeMcpSettingsWithPreset(
  settingsDir: string,
  projectRoot: string,
  presetName?: string,
): Promise<McpSettings> {
  const presetDir = getDefaultMcpPresetDir(projectRoot);
  const indexPath = path.join(presetDir, "index.json");

  // Check if preset index exists
  let baseSettings: McpSettings | null = null;
  try {
    await fs.access(indexPath);
  } catch {
    // No preset index → fall back to legacy loading
    return loadAndMergeMcpSettings(settingsDir, projectRoot);
  }

  // Preset mode: load the base preset
  if (presetName) {
    try {
      baseSettings = await loadMcpPresetSettings(presetDir, presetName);
    } catch (error) {
      // Preset not found, fall back to legacy
      return loadAndMergeMcpSettings(settingsDir, projectRoot);
    }
  }

  // Merge with legacy global/project overrides
  const projectPaths = getProjectMcpPaths(projectRoot);

  const [globalSettings, ...projectSettings] = await Promise.all([
    loadMcpSettings(getGlobalMcpSettingsPath(settingsDir)).catch(() => null),
    ...projectPaths.map((p) => loadMcpSettings(p).catch(() => null)),
  ]);

  const base = baseSettings ?? globalSettings ?? createDefaultMcpSettings();
  const baseConfigs = loadServerConfigs(base).configs;

  // Merge: project overrides > global > preset base
  let mergedConfigs = new Map(baseConfigs);

  // Apply global as override if preset was used as base
  if (baseSettings && globalSettings) {
    const globalConfigs = loadServerConfigs(globalSettings).configs;
    mergedConfigs = mergeServerConfigs(mergedConfigs, globalConfigs);
  }

  for (const settings of projectSettings) {
    if (settings !== null) {
      const projectConfigs = loadServerConfigs(settings).configs;
      mergedConfigs = mergeServerConfigs(mergedConfigs, projectConfigs);
    }
  }

  return {
    mcpServers: Object.fromEntries(mergedConfigs),
  };
}