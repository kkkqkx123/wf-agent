/**
 * Preset Loader
 *
 * Generic preset/index-based configuration loading for MCP, Skill, and
 * Infrastructure config types.
 *
 * Preset resolution flow:
 *   1. Load `index.json` from the preset directory → get path patterns
 *   2. Expand glob patterns → discover all `*.json` preset files
 *   3. Index presets by filename (without extension) → name → filePath map
 *   4. Look up the requested preset name → load the matched file
 *   5. Return the parsed preset content
 *
 * Two preset types:
 *   - **Single-file preset** (MCP): each preset file IS the config.
 *   - **Multi-file preset** (Skill/Infrastructure): each preset file contains
 *     `paths` or `files` mappings that point to the actual config files.
 */

import * as fs from "fs/promises";
import * as path from "path";
import type { ConfigIndexFile } from "@wf-agent/types";
import { INDEX_FILE_NAMES } from "@wf-agent/types";
import { expandIndexPaths } from "./config-index-loader.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * Indexed preset entry: maps a preset name to its file path.
 */
export interface PresetEntry {
  /** Preset name (derived from filename without extension) */
  name: string;
  /** Absolute path to the preset definition file */
  filePath: string;
}

/**
 * Result of resolving a preset index.
 */
export interface ResolvedPresetIndex {
  /** All discovered presets indexed by name */
  presets: Map<string, PresetEntry>;
  /** Files that failed to resolve */
  failures: Array<{ path: string; error: string }>;
}

// ---------------------------------------------------------------------------
// Preset Index Resolution
// ---------------------------------------------------------------------------

/**
 * Load and resolve a preset index.
 *
 * Steps:
 * 1. Find and parse the `index.json` in the given directory
 * 2. Expand its `paths` glob patterns
 * 3. Index all matched `*.json` files by filename (no ext)
 *
 * @param baseDir - Directory containing the preset index (e.g. `configs/mcp`)
 * @returns Resolved preset index with name → filePath mappings
 */
export async function resolvePresetIndex(
  baseDir: string,
): Promise<ResolvedPresetIndex> {
  const indexPath = path.join(baseDir, INDEX_FILE_NAMES["mcp_presets"]);

  let index: ConfigIndexFile;
  try {
    const content = await fs.readFile(indexPath, "utf-8");
    index = JSON.parse(content) as ConfigIndexFile;
  } catch (error) {
    throw new Error(
      `Failed to load preset index at ${indexPath}`,
      { cause: error },
    );
  }

  const filePaths = await expandIndexPaths(index, baseDir);

  const presets = new Map<string, PresetEntry>();
  const failures: Array<{ path: string; error: string }> = [];

  for (const filePath of filePaths) {
    // Skip the index file itself when using broad patterns like ./*.json
    if (path.basename(filePath).toLowerCase() === INDEX_FILE_NAMES["mcp_presets"]) continue;

    const name = path.basename(filePath, path.extname(filePath));
    if (!name) continue;

    if (presets.has(name)) {
      failures.push({
        path: filePath,
        error: `Duplicate preset name "${name}" (conflicts with ${presets.get(name)!.filePath})`,
      });
      continue;
    }

    presets.set(name, { name, filePath });
  }

  return { presets, failures };
}

// ---------------------------------------------------------------------------
// Preset Lookup
// ---------------------------------------------------------------------------

/**
 * Find a preset entry by name.
 *
 * @param presets - Map of preset name → PresetEntry
 * @param name - Preset name to look up
 * @returns The preset entry, or null if not found
 */
export function findPresetByName(
  presets: Map<string, PresetEntry>,
  name: string,
): PresetEntry | null {
  return presets.get(name) ?? null;
}

/**
 * List all available preset names.
 */
export function listPresetNames(
  presets: Map<string, PresetEntry>,
): string[] {
  return Array.from(presets.keys());
}

// ---------------------------------------------------------------------------
// Preset Loading
// ---------------------------------------------------------------------------

/**
 * Load and parse a single-file preset (e.g. MCP preset).
 *
 * @param entry - The preset entry to load
 * @returns Parsed JSON content
 */
export async function loadSingleFilePreset<T>(
  entry: PresetEntry,
): Promise<T> {
  const content = await fs.readFile(entry.filePath, "utf-8");

  try {
    return JSON.parse(content) as T;
  } catch (error) {
    throw new Error(
      `Failed to parse preset file: ${entry.filePath}`,
      { cause: error },
    );
  }
}

