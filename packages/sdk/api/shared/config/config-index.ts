/**
 * Load Configuration Index
 *
 * General-purpose API for loading configuration index files.
 *
 * Implementation:
 * - API contract defined here in SDK (pure, no I/O)
 * - Actual file I/O and resolvers provided by packages/config-processor
 * - Resolvers registered during application initialization
 *
 * Design:
 * - Uses registration pattern for loose coupling between SDK and implementations
 * - Supports 9 index types (see IndexType)
 * - Parallel loading via loadMultipleConfigIndexes()
 *
 * Usage:
 * ```ts
 * import { registerAllIndexResolvers } from "@wf-agent/config-processor";
 * import { loadConfigIndex } from "@wf-agent/sdk/api";
 *
 * // During app initialization
 * registerAllIndexResolvers();
 *
 * // Load an index
 * const workflowIndex = await loadConfigIndex(
 *   "workflows",
 *   "./configs/workflows/index.json"
 * );
 *
 * console.log(`Loaded ${workflowIndex.entries.length} workflows`);
 * workflowIndex.entries.forEach(entry => {
 *   console.log(`  - ${entry.id}: ${entry.name}`);
 * });
 * ```
 */

import type {
  ResolvedIndex,
  ResolvedIndexEntry,
  ResolvedLLMProfileEntry,
  ResolvedWorkflowEntry,
  ResolvedNodeTemplateEntry,
  ResolvedScriptEntry,
} from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

/**
 * Supported index types for loadConfigIndex.
 */
export type IndexType =
  | "llm_profiles"
  | "workflows"
  | "node_templates"
  | "scripts"
  | "prompt_templates"
  | "agent_loops"
  | "mcp_presets"
  | "skill_presets"
  | "infrastructure_presets";

/**
 * Function signature for index resolvers.
 */
export type IndexResolver = (
  indexPath: string,
) => Promise<ResolvedIndex<ResolvedIndexEntry>>;

/**
 * Mapping of index types to their resolve functions.
 * Actual implementations are provided by apps/config-processor.
 */
const logger = createContextualLogger({ component: "ConfigIndex" });

const RESOLVE_FUNCTIONS: Partial<Record<IndexType, IndexResolver>> = {};

/**
 * Register a resolver function for an index type.
 * Used by apps/config-processor to provide actual implementations.
 */
export function registerResolver(
  type: IndexType,
  resolver: IndexResolver,
): void {
  RESOLVE_FUNCTIONS[type] = resolver;
}

/**
 * Load a configuration index by type and path.
 *
 * @param type - Type of index to load
 * @param indexPath - Path to the index file or its parent directory
 * @returns Resolved index with entries and metadata
 * @throws Error if index type is not supported or file cannot be loaded
 */
export async function loadConfigIndex(
  type: IndexType,
  indexPath: string,
): Promise<ResolvedIndex<ResolvedIndexEntry>> {
  const resolveFn = RESOLVE_FUNCTIONS[type];

  if (!resolveFn) {
    throw new Error(
      `Unsupported or unregistered index type: "${type}". ` +
        `Supported types: ${Object.keys(RESOLVE_FUNCTIONS).join(", ")}. ` +
        `Use registerResolver() to register a resolver.`,
    );
  }

  return resolveFn(indexPath);
}

/**
 * Load multiple configuration indexes in parallel.
 *
 * @param configs - Array of type and path pairs
 * @returns Map of index type to resolved index
 */
export async function loadMultipleConfigIndexes(
  configs: Array<{ type: IndexType; path: string }>,
): Promise<Map<IndexType, ResolvedIndex<ResolvedIndexEntry>>> {
  const results = new Map<IndexType, ResolvedIndex<ResolvedIndexEntry>>();

  const promises = configs.map(async ({ type, path }) => {
    try {
      const index = await loadConfigIndex(type, path);
      results.set(type, index);
    } catch (error) {
      logger.error(
        `Failed to load config index (${type}): ${error instanceof Error ? error.message : error}`,
      );
      throw error;
    }
  });

  await Promise.all(promises);

  return results;
}

/**
 * Check if a resolver is registered for the given index type.
 *
 * @param type - Type of index to check
 * @returns true if a resolver is registered
 */
export function hasResolver(type: IndexType): boolean {
  return type in RESOLVE_FUNCTIONS;
}

/**
 * List all registered index types.
 */
export function listIndexTypes(): IndexType[] {
  return Object.keys(RESOLVE_FUNCTIONS) as IndexType[];
}

/**
 * Utility type for extracting entry type from index type.
 */
export type IndexEntryType<T extends IndexType> =
  T extends "llm_profiles"
    ? ResolvedLLMProfileEntry
    : T extends "workflows"
      ? ResolvedWorkflowEntry
      : T extends "node_templates"
        ? ResolvedNodeTemplateEntry
        : T extends "scripts"
          ? ResolvedScriptEntry
          : ResolvedIndexEntry;