/**
 * Tool Schema Helper
 *
 * Provides helper functions for working with tool schemas.
 * Supports schema validation, transformation, and extraction.
 *
 * @remarks
 * The tool schema helper provides a set of utility functions for working with
 * tool schemas. It includes functions for schema validation, transformation,
 * and extraction of specific properties from tool definitions.
 */

import type { Tool } from "@wf-agent/types";

/**
 * Prepare tool schemas from a list of tools
 *
 * @param tools - The tools to prepare schemas for
 * @returns An array of tool schemas
 */
export function prepareToolSchemasFromTools(tools: Tool[]): Tool[] {
  return tools;
}

/**
 * Extract tool names from a list of tool schemas
 *
 * @param tools - The tools to extract names from
 * @returns An array of tool names
 */
export function extractToolNames(tools: Tool[]): string[] {
  return tools.map(tool => tool.id);
}

/**
 * Find a tool by name in a list of tools
 *
 * @param tools - The tools to search
 * @param toolName - The name of the tool to find
 * @returns The found tool, or undefined
 */
export function findToolByName(tools: Tool[], toolName: string): Tool | undefined {
  return tools.find(tool => tool.id === toolName);
}

/**
 * Check if a tool exists in a list of tools
 *
 * @param tools - The tools to search
 * @param toolName - The name of the tool to check
 * @returns True if the tool exists
 */
export function hasTool(tools: Tool[], toolName: string): boolean {
  return tools.some(tool => tool.id === toolName);
}

/**
 * Filter tools by category
 *
 * @param tools - The tools to filter
 * @param category - The category to filter by
 * @returns An array of matching tools
 */
export function filterToolsByCategory(
  tools: Tool[],
  category: string,
): Tool[] {
  return tools.filter(tool => tool.id.startsWith(category));
}

/** @deprecated Use `prepareToolSchemasFromTools` instead */
export const prepareToolSchemas = prepareToolSchemasFromTools;