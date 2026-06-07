/**
 * MCP Configuration Loader
 *
 * File I/O operations for MCP server configuration management.
 * All pure processing/validation logic lives in processors/mcp-connection.ts.
 */

import * as fs from "fs/promises";
import * as path from "path";
import type { McpSettings } from "@wf-agent/types";
import {
  validateMcpSettings,
  createDefaultMcpSettings,
} from "../processors/mcp-connection.js";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/**
 * Default MCP settings file name
 */
export const DEFAULT_MCP_SETTINGS_FILE = "mcp-settings.json";

/**
 * Default project MCP file path
 */
export const PROJECT_MCP_FILE = ".agent/mcp.json";

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
 * Get project MCP file path
 */
export function getProjectMcpPath(projectRoot: string): string {
  return path.join(projectRoot, PROJECT_MCP_FILE);
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/**
 * Check if a file exists
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

  return validateMcpSettings(config);
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