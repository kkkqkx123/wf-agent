/**
 * Tool Description Registry
 *
 * An in-memory registry for caching tool descriptions.
 * Provides fast lookup of tool descriptions without re-computing them.
 *
 * @remarks
 * The tool description registry is used to avoid re-generating tool descriptions
 * for tools that have already been processed. It maintains a cache of tool
 * descriptions keyed by tool ID, and provides methods for registering, retrieving,
 * and clearing tool descriptions.
 *
 * This is particularly useful for workflows that reuse the same tools across
 * multiple iterations or conversations.
 */

import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { Tool, ToolDescriptionData } from "@wf-agent/types";
import { generateToolDescription } from "./tool-description-generator.js";

const logger = createContextualLogger({ component: "ToolDescriptionRegistry" });

/**
 * In-memory cache for tool descriptions, keyed by tool ID
 */
const descriptionCache = new Map<string, string>();

/**
 * Register a tool's description in the cache
 *
 * @param tool - The tool to register
 * @returns The generated description
 */
export function registerToolDescription(tool: Tool): string {
  const { id } = tool;

  logger.debug(`Registering tool description for tool: ${id}`);

  if (descriptionCache.has(id)) {
    logger.debug(`Found existing description for tool: ${id}`);
    return descriptionCache.get(id)!;
  }

  const description = generateToolDescription(tool);
  descriptionCache.set(id, description);
  logger.debug(`Registered new description for tool: ${id}`);
  return description;
}

/**
 * Get a tool's description from the cache
 *
 * @param toolId - The tool ID to look up
 * @returns The cached description, or undefined if not found
 */
export function getToolDescription(toolId: string): string | undefined {
  return descriptionCache.get(toolId);
}

/**
 * Check if a tool description exists in the cache
 *
 * @param toolId - The tool ID to check
 * @returns True if the description exists in the cache
 */
export function hasToolDescription(toolId: string): boolean {
  return descriptionCache.has(toolId);
}

/**
 * Clear the tool description cache
 */
export function clearToolDescriptionCache(): void {
  logger.debug("Clearing tool description cache");
  descriptionCache.clear();
}

/**
 * Get the size of the tool description cache
 *
 * @returns The number of cached tool descriptions
 */
export function getToolDescriptionCacheSize(): number {
  return descriptionCache.size;
}

/**
 * Register a batch of tool descriptions
 *
 * @param tools - The tools to register
 * @returns An array of generated descriptions
 */
export function registerToolDescriptions(tools: Tool[]): string[] {
  return tools.map(tool => registerToolDescription(tool));
}

/**
 * Tool Description Registry type
 */
export interface ToolDescriptionRegistry {
  register(tool: Tool | ToolDescriptionData): string;
  has(toolId: string): boolean;
  get(toolId: string): string | undefined;
  clear(): void;
  size(): number;
  registerAll(tools: Tool[]): string[];
}

/**
 * Global tool description registry instance
 */
export const toolDescriptionRegistry: ToolDescriptionRegistry = {
  register: (tool) => registerToolDescription(tool as Tool),
  has: (toolId) => hasToolDescription(toolId),
  get: (toolId) => getToolDescription(toolId),
  clear: () => clearToolDescriptionCache(),
  size: () => getToolDescriptionCacheSize(),
  registerAll: (tools) => registerToolDescriptions(tools),
};