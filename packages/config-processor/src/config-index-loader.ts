/**
 * Configuration Index Loader
 *
 * Provides functions for loading configuration index files and
 * batch loading configurations based on path patterns.
 *
 * Design:
 * - Index files contain ONLY path patterns (minimal and simple)
 * - Metadata is extracted from individual config files
 * - Supports glob patterns for flexible path matching
 */

import * as fs from "fs/promises";
import * as path from "path";
import { matchGlobPattern } from "@wf-agent/common-utils";
import type {
  ConfigIndexFile,
  ResolvedIndex,
  ResolvedIndexEntry,
  ResolvedLLMProfileEntry,
  ResolvedWorkflowEntry,
  ResolvedNodeTemplateEntry,
  ResolvedScriptEntry,
} from "@wf-agent/types";
import { INDEX_FILE_NAMES } from "@wf-agent/types";
import {
  loadLLMProfileConfig,
  loadNodeTemplateConfig,
  loadScriptConfig,
  loadPromptTemplateConfig,
  loadAgentLoopConfig,
} from "./loader-orchestrator.js";

/**
 * Expand all glob patterns in the index file.
 *
 * @param index - The index file
 * @param indexDir - Directory containing the index file
 * @returns Array of absolute file paths
 */
export async function expandIndexPaths(index: ConfigIndexFile, indexDir: string): Promise<string[]> {
  const allPaths: string[] = [];

  for (const pattern of index.paths) {
    const matches = await matchGlobPattern(pattern, indexDir);
    allPaths.push(...matches);
  }

  // Deduplicate
  return [...new Set(allPaths)];
}

// ---------------------------------------------------------------------------
// Metadata Extraction
// ---------------------------------------------------------------------------

/**
 * Extract metadata from an LLM Profile config.
 */
function extractLLMProfileMetadata(config: unknown): Partial<ResolvedLLMProfileEntry> {
  const profile = config as Record<string, unknown>;
  return {
    id: String(profile["id"] || ""),
    name: profile["name"] ? String(profile["name"]) : undefined,
    description: profile["description"] ? String(profile["description"]) : undefined,
    tags: Array.isArray(profile["tags"]) ? (profile["tags"] as string[]) : undefined,
    provider: profile["provider"] ? String(profile["provider"]) : undefined,
    model: profile["model"] ? String(profile["model"]) : undefined,
  };
}

/**
 * Extract metadata from a Workflow config.
 */
function extractWorkflowMetadata(config: unknown): Partial<ResolvedWorkflowEntry> {
  const workflow = config as Record<string, unknown>;
  return {
    id: String(workflow["id"] || ""),
    name: workflow["name"] ? String(workflow["name"]) : undefined,
    description: workflow["description"] ? String(workflow["description"]) : undefined,
    tags: Array.isArray(workflow["tags"]) ? (workflow["tags"] as string[]) : undefined,
    type: workflow["type"] ? String(workflow["type"]) : undefined,
    version: workflow["version"] ? String(workflow["version"]) : undefined,
    author: workflow["author"] ? String(workflow["author"]) : undefined,
  };
}

/**
 * Extract metadata from a Node Template config.
 */
function extractNodeTemplateMetadata(config: unknown): Partial<ResolvedNodeTemplateEntry> {
  const template = config as Record<string, unknown>;
  return {
    id: String(template["name"] || ""), // Node templates use 'name' as ID
    name: template["name"] ? String(template["name"]) : undefined,
    description: template["description"] ? String(template["description"]) : undefined,
    tags: Array.isArray(template["tags"]) ? (template["tags"] as string[]) : undefined,
    type: template["type"] ? String(template["type"]) : undefined,
  };
}

/**
 * Extract metadata from a Script config.
 */
function extractScriptMetadata(config: unknown): Partial<ResolvedScriptEntry> {
  const script = config as Record<string, unknown>;
  return {
    id: String(script["id"] || script["name"] || ""),
    name: script["name"] ? String(script["name"]) : undefined,
    description: script["description"] ? String(script["description"]) : undefined,
    tags: Array.isArray(script["tags"]) ? (script["tags"] as string[]) : undefined,
    category: script["category"] ? String(script["category"]) : undefined,
  };
}

/**
 * Extract metadata from a Prompt Template config.
 */
function extractPromptTemplateMetadata(config: unknown): Partial<ResolvedIndexEntry> {
  const template = config as Record<string, unknown>;
  return {
    id: String(template["id"] || ""),
    name: template["name"] ? String(template["name"]) : undefined,
    description: template["description"] ? String(template["description"]) : undefined,
    tags: Array.isArray(template["tags"]) ? (template["tags"] as string[]) : undefined,
    category: template["category"] ? String(template["category"]) : undefined,
  };
}

/**
 * Extract metadata from an Agent Loop config.
 */
function extractAgentLoopMetadata(config: unknown): Partial<ResolvedIndexEntry> {
  const agentLoop = config as Record<string, unknown>;
  return {
    id: String(agentLoop["id"] || ""),
    name: agentLoop["name"] ? String(agentLoop["name"]) : undefined,
    description: agentLoop["description"] ? String(agentLoop["description"]) : undefined,
    tags: Array.isArray(agentLoop["tags"]) ? (agentLoop["tags"] as string[]) : undefined,
  };
}

// ---------------------------------------------------------------------------
// Index File Loading
// ---------------------------------------------------------------------------

/**
 * Load a configuration index file.
 *
 * @param indexPath - Path to the index file or its parent directory.
 * @returns The loaded index file.
 * @throws Error if the index file cannot be read or parsed.
 */
export async function loadIndexFile(indexPath: string): Promise<ConfigIndexFile> {
  // If path is a directory, append index filename
  const stats = await fs.stat(indexPath);
  const filePath = stats.isDirectory()
    ? path.join(indexPath, INDEX_FILE_NAMES["llm_profiles"])
    : indexPath;

  const content = await fs.readFile(filePath, "utf-8");

  try {
    const index = JSON.parse(content) as ConfigIndexFile;
    return index;
  } catch (error) {
    throw new Error(
      `Failed to parse index file: ${filePath}. Invalid JSON syntax.`,
      { cause: error },
    );
  }
}

/**
 * Try to load an index file, returning null if not found.
 */
export async function tryLoadIndexFile(indexPath: string): Promise<ConfigIndexFile | null> {
  try {
    return await loadIndexFile(indexPath);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

/**
 * Check if an index file exists.
 */
export async function indexFileExists(indexPath: string): Promise<boolean> {
  try {
    const stats = await fs.stat(indexPath);
    const filePath = stats.isDirectory()
      ? path.join(indexPath, INDEX_FILE_NAMES["llm_profiles"])
      : indexPath;
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Index Resolution (Load + Extract Metadata)
// ---------------------------------------------------------------------------

/**
 * Resolve an LLM Profile index.
 * Loads all config files and extracts metadata.
 */
export async function resolveLLMProfileIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedLLMProfileEntry>> {
  const index = await loadIndexFile(indexPath);
  const indexDir = path.dirname(indexPath);
  const filePaths = await expandIndexPaths(index, indexDir);

  const entries: ResolvedLLMProfileEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  for (const filePath of filePaths) {
    try {
      const parsed = await loadLLMProfileConfig(filePath);
      const metadata = extractLLMProfileMetadata(parsed.config);
      const ext = path.extname(filePath).toLowerCase();

      entries.push({
        ...metadata,
        filePath,
        format: ext === ".json" ? "json" : "toml",
      } as ResolvedLLMProfileEntry);
    } catch (error) {
      failures.push({
        path: filePath,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return {
    type: "llm_profiles",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}

/**
 * Resolve a Workflow index.
 */
export async function resolveWorkflowIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedWorkflowEntry>> {
  const index = await loadIndexFile(indexPath);
  const indexDir = path.dirname(indexPath);
  const filePaths = await expandIndexPaths(index, indexDir);

  const entries: ResolvedWorkflowEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  for (const filePath of filePaths) {
    try {
      const parsed = await loadNodeTemplateConfig(filePath);
      const metadata = extractWorkflowMetadata(parsed.config);
      const ext = path.extname(filePath).toLowerCase();

      entries.push({
        ...metadata,
        filePath,
        format: ext === ".json" ? "json" : "toml",
      } as ResolvedWorkflowEntry);
    } catch (error) {
      failures.push({
        path: filePath,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return {
    type: "workflows",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}

/**
 * Resolve a Node Template index.
 */
export async function resolveNodeTemplateIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedNodeTemplateEntry>> {
  const index = await loadIndexFile(indexPath);
  const indexDir = path.dirname(indexPath);
  const filePaths = await expandIndexPaths(index, indexDir);

  const entries: ResolvedNodeTemplateEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  for (const filePath of filePaths) {
    try {
      const parsed = await loadNodeTemplateConfig(filePath);
      const metadata = extractNodeTemplateMetadata(parsed.config);
      const ext = path.extname(filePath).toLowerCase();

      entries.push({
        ...metadata,
        filePath,
        format: ext === ".json" ? "json" : "toml",
      } as ResolvedNodeTemplateEntry);
    } catch (error) {
      failures.push({
        path: filePath,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return {
    type: "node_templates",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}

/**
 * Resolve a Script index.
 */
export async function resolveScriptIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedScriptEntry>> {
  const index = await loadIndexFile(indexPath);
  const indexDir = path.dirname(indexPath);
  const filePaths = await expandIndexPaths(index, indexDir);

  const entries: ResolvedScriptEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  for (const filePath of filePaths) {
    try {
      const parsed = await loadScriptConfig(filePath);
      const metadata = extractScriptMetadata(parsed.config);
      const ext = path.extname(filePath).toLowerCase();

      entries.push({
        ...metadata,
        filePath,
        format: ext === ".json" ? "json" : "toml",
      } as ResolvedScriptEntry);
    } catch (error) {
      failures.push({
        path: filePath,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return {
    type: "scripts",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}

/**
 * Resolve a Prompt Template index.
 */
export async function resolvePromptTemplateIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedIndexEntry>> {
  const index = await loadIndexFile(indexPath);
  const indexDir = path.dirname(indexPath);
  const filePaths = await expandIndexPaths(index, indexDir);

  const entries: ResolvedIndexEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  for (const filePath of filePaths) {
    try {
      const parsed = await loadPromptTemplateConfig(filePath);
      const metadata = extractPromptTemplateMetadata(parsed.config);
      const ext = path.extname(filePath).toLowerCase();

      entries.push({
        ...metadata,
        filePath,
        format: ext === ".json" ? "json" : "toml",
      } as ResolvedIndexEntry);
    } catch (error) {
      failures.push({
        path: filePath,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return {
    type: "prompt_templates",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}

/**
 * Resolve an Agent Loop index.
 */
export async function resolveAgentLoopIndex(
  indexPath: string,
): Promise<ResolvedIndex<ResolvedIndexEntry>> {
  const index = await loadIndexFile(indexPath);
  const indexDir = path.dirname(indexPath);
  const filePaths = await expandIndexPaths(index, indexDir);

  const entries: ResolvedIndexEntry[] = [];
  const failures: Array<{ path: string; error: string }> = [];

  for (const filePath of filePaths) {
    try {
      const parsed = await loadAgentLoopConfig(filePath);
      const metadata = extractAgentLoopMetadata(parsed.config);
      const ext = path.extname(filePath).toLowerCase();

      entries.push({
        ...metadata,
        filePath,
        format: ext === ".json" ? "json" : "toml",
      } as ResolvedIndexEntry);
    } catch (error) {
      failures.push({
        path: filePath,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return {
    type: "agent_loops",
    entries,
    metadata: {
      resolvedAt: new Date().toISOString(),
      totalCount: entries.length,
      failures,
    },
  };
}

// ---------------------------------------------------------------------------
// Filtering Utilities
// ---------------------------------------------------------------------------

/**
 * Filter resolved entries by tags.
 */
export function filterByTags<T extends ResolvedIndexEntry>(
  entries: T[],
  tags: string[],
): T[] {
  if (tags.length === 0) return entries;
  return entries.filter((entry) =>
    tags.every((tag) => entry.tags?.includes(tag)),
  );
}

/**
 * Filter resolved entries by category.
 */
export function filterByCategory<T extends ResolvedIndexEntry>(
  entries: T[],
  category: string,
): T[] {
  return entries.filter((entry) => entry.category === category);
}

/**
 * Find an entry by ID.
 */
export function findEntryById<T extends ResolvedIndexEntry>(
  entries: T[],
  id: string,
): T | undefined {
  return entries.find((entry) => entry.id === id);
}
