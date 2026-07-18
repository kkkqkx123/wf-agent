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
 * Tool Description Registry interface
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
 * Tool Description Registry Class
 *
 * In-memory cache for tool descriptions, keyed by tool ID.
 * Can be instantiated as a standalone instance or obtained via the global singleton.
 */
export class ToolDescriptionRegistryImpl implements ToolDescriptionRegistry {
  private cache = new Map<string, string>();

  /**
   * Register a tool's description in the cache.
   *
   * @param tool - The tool to register
   * @returns The generated description
   */
  register(tool: Tool | ToolDescriptionData): string {
    const { id } = tool;

    logger.debug(`Registering tool description for tool: ${id}`);

    if (this.cache.has(id)) {
      logger.debug(`Found existing description for tool: ${id}`);
      return this.cache.get(id)!;
    }

    const description = generateToolDescription(tool as Tool);
    this.cache.set(id, description);
    logger.debug(`Registered new description for tool: ${id}`);
    return description;
  }

  /**
   * Get a tool's description from the cache.
   *
   * @param toolId - The tool ID to look up
   * @returns The cached description, or undefined if not found
   */
  get(toolId: string): string | undefined {
    return this.cache.get(toolId);
  }

  /**
   * Check if a tool description exists in the cache.
   *
   * @param toolId - The tool ID to check
   * @returns True if the description exists in the cache
   */
  has(toolId: string): boolean {
    return this.cache.has(toolId);
  }

  /**
   * Clear the tool description cache.
   */
  clear(): void {
    logger.debug("Clearing tool description cache");
    this.cache.clear();
  }

  /**
   * Get the number of cached tool descriptions.
   *
   * @returns The number of cached tool descriptions
   */
  size(): number {
    return this.cache.size;
  }

  /**
   * Register a batch of tool descriptions.
   *
   * @param tools - The tools to register
   * @returns An array of generated descriptions
   */
  registerAll(tools: Tool[]): string[] {
    return tools.map(tool => this.register(tool));
  }
}

/**
 * Global tool description registry singleton instance.
 */
export const toolDescriptionRegistry: ToolDescriptionRegistry = new ToolDescriptionRegistryImpl();

/**
 * @deprecated Use `ToolDescriptionRegistryImpl` class directly instead.
 * These standalone functions operate on the global singleton for backward compatibility.
 */

/** @deprecated Use `toolDescriptionRegistry.register()` instead. */
export function registerToolDescription(tool: Tool): string {
  return toolDescriptionRegistry.register(tool);
}

/** @deprecated Use `toolDescriptionRegistry.get()` instead. */
export function getToolDescription(toolId: string): string | undefined {
  return toolDescriptionRegistry.get(toolId);
}

/** @deprecated Use `toolDescriptionRegistry.has()` instead. */
export function hasToolDescription(toolId: string): boolean {
  return toolDescriptionRegistry.has(toolId);
}

/** @deprecated Use `toolDescriptionRegistry.clear()` instead. */
export function clearToolDescriptionCache(): void {
  toolDescriptionRegistry.clear();
}

/** @deprecated Use `toolDescriptionRegistry.size()` instead. */
export function getToolDescriptionCacheSize(): number {
  return toolDescriptionRegistry.size();
}

/** @deprecated Use `toolDescriptionRegistry.registerAll()` instead. */
export function registerToolDescriptions(tools: Tool[]): string[] {
  return toolDescriptionRegistry.registerAll(tools);
}