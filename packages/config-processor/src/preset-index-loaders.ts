/**
 * Preset-based Index Loaders
 *
 * Loaders for MCP Presets, Skill Presets, and Infrastructure Presets.
 * Unlike standard config indices which load from path patterns in index files,
 * these loaders work with preset configuration files that define collections.
 */

import * as path from "path";
import { matchGlobPattern } from "@wf-agent/common-utils";
import type { ResolvedIndex, ResolvedIndexEntry } from "@wf-agent/types";
import { loadMcpSettings } from "./mcp-settings-loader.js";
import { loadSkillConfig } from "./skill-settings-loader.js";
import { loadInfrastructureConfigs } from "./loader-orchestrator.js";

const logger = {
  error: (msg: string, err?: unknown) => console.error(msg, err),
};

// ---------------------------------------------------------------------------
// MCP Presets Index
// ---------------------------------------------------------------------------

/**
 * Resolve a MCP Presets index.
 * Loads MCP settings files and creates entries for each configured server.
 *
 * @param indexPath - Path to the MCP presets index file or directory
 * @returns Resolved index with MCP server entries
 */
export async function resolveMcpPresetsIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedIndexEntry>> {
  const entries: ResolvedIndexEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  try {
    // Try to load MCP settings from the path
    const settings = await loadMcpSettings(indexPath);

    // Create an entry for each configured MCP server
    if (settings.mcpServers) {
      for (const [serverId] of Object.entries(settings.mcpServers)) {
        entries.push({
          id: serverId,
          name: serverId,
          filePath: indexPath,
          format: indexPath.endsWith(".json") ? "json" : "toml",
        });
      }
    }
  } catch (error) {
    failures.push({
      path: indexPath,
      error: error instanceof Error ? error.message : String(error),
    });
  }

  return {
    type: "mcp_presets",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}

// ---------------------------------------------------------------------------
// Skill Presets Index
// ---------------------------------------------------------------------------

/**
 * Resolve a Skill Presets index.
 * Loads skill configuration files and creates entries for each skill collection.
 *
 * @param indexPath - Path to the skill presets index file or directory
 * @returns Resolved index with skill collection entries
 */
export async function resolveSkillPresetsIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedIndexEntry>> {
  const entries: ResolvedIndexEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  try {
    // Try to load skill config from the path
    const config = await loadSkillConfig(indexPath);

    if (config && config.paths && config.paths.length > 0) {
      // The skill configuration defines paths to skill files
      // Create an entry for the skill configuration itself
      const baseName = path.basename(
        indexPath,
        path.extname(indexPath),
      );

      entries.push({
        id: baseName,
        name: baseName,
        description: `Skill collection from ${baseName}`,
        filePath: indexPath,
        format: indexPath.endsWith(".json") ? "json" : "toml",
      });

      // Optionally, expand paths and create entries for individual skills
      const skillDir = path.dirname(indexPath);
      try {
        const skillPaths = await expandSkillPaths(config.paths, skillDir);

        for (const skillPath of skillPaths) {
          const skillName = path.basename(
            skillPath,
            path.extname(skillPath),
          );
          entries.push({
            id: `${baseName}:${skillName}`,
            name: skillName,
            description: `Skill from ${baseName}`,
            filePath: skillPath,
            format: skillPath.endsWith(".json") ? "json" : "toml",
          });
        }
      } catch (error) {
        // If we can't expand paths, just use the config entry
        logger.error(`Failed to expand skill paths: ${error}`);
      }
    }
  } catch (error) {
    failures.push({
      path: indexPath,
      error: error instanceof Error ? error.message : String(error),
    });
  }

  return {
    type: "skill_presets",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}

/**
 * Expand skill path patterns to actual file paths.
 */
async function expandSkillPaths(
  patterns: string[],
  baseDir: string,
): Promise<string[]> {
  const allPaths: string[] = [];

  for (const pattern of patterns) {
    try {
      const matches = await matchGlobPattern(pattern, baseDir);
      allPaths.push(...matches);
    } catch (error) {
      logger.error(`Failed to match pattern ${pattern}:`, error);
    }
  }

  return [...new Set(allPaths)];
}

// ---------------------------------------------------------------------------
// Infrastructure Presets Index
// ---------------------------------------------------------------------------

/**
 * Resolve an Infrastructure Presets index.
 * Loads infrastructure preset configuration files.
 *
 * @param indexPath - Path to the infrastructure presets index file or directory
 * @returns Resolved index with infrastructure preset entries
 */
export async function resolveInfrastructurePresetsIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedIndexEntry>> {
  const entries: ResolvedIndexEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  try {
    // Extract project root from the indexPath
    const projectRoot = path.dirname(indexPath);

    // Try to load infrastructure config
    await loadInfrastructureConfigs(projectRoot);

    // Create an entry for the infrastructure preset
    const presetId = path.basename(
      indexPath,
      path.extname(indexPath),
    );

    entries.push({
      id: presetId,
      name: presetId,
      filePath: indexPath,
      format: indexPath.endsWith(".json") ? "json" : "toml",
    });
  } catch (error) {
    failures.push({
      path: indexPath,
      error: error instanceof Error ? error.message : String(error),
    });
  }

  return {
    type: "infrastructure_presets",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}


