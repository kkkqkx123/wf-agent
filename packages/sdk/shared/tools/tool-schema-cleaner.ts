/**
 * Tool Schema Cleaner
 *
 * Provides utilities for cleaning and normalizing tool schemas.
 * Removes unwanted properties and ensures consistent schema format.
 *
 * @remarks
 * The tool schema cleaner is used to remove properties from tool schemas
 * that are not needed for LLM consumption, such as metadata, internal
 * identifiers, or implementation details. It also normalizes the schema
 * format to ensure consistency across different tool definitions.
 */

import type { Tool, ToolProperty } from "@wf-agent/types";

/**
 * Clean a tool schema by removing unwanted properties
 *
 * @param tool - The tool to clean
 * @param propertiesToRemove - An array of property names to remove from the schema
 * @returns A new tool object with the specified properties removed
 */
export function cleanToolSchema(
  tool: Tool,
  propertiesToRemove: string[] = [],
): Tool {
  if (!tool.parameters || !tool.parameters.properties) {
    return tool;
  }

  const cleanedProperties = { ...tool.parameters.properties };

  for (const prop of propertiesToRemove) {
    delete cleanedProperties[prop];
  }

  return {
    ...tool,
    parameters: {
      ...tool.parameters,
      properties: cleanedProperties,
    },
  };
}

/**
 * Remove all properties except those specified
 *
 * @param tool - The tool to filter
 * @param propertiesToKeep - An array of property names to keep
 * @returns A new tool object with only the specified properties
 */
export function keepToolSchemaProperties(
  tool: Tool,
  propertiesToKeep: string[] = [],
): Tool {
  if (!tool.parameters || !tool.parameters.properties) {
    return tool;
  }

  const filteredProperties: Record<string, ToolProperty> = {};

  for (const prop of propertiesToKeep) {
    if (prop in tool.parameters.properties) {
      const val = tool.parameters.properties[prop];
      if (val !== undefined) {
        filteredProperties[prop] = val;
      }
    }
  }

  return {
    ...tool,
    parameters: {
      ...tool.parameters,
      properties: filteredProperties,
    },
  };
}

/**
 * Remove description fields from a tool schema
 *
 * @param tool - The tool to clean
 * @returns A new tool object with descriptions removed
 */
export function removeDescriptionsFromSchema(tool: Tool): Tool {
  if (!tool.parameters || !tool.parameters.properties) {
    return tool;
  }

  const cleanedProperties: Record<string, ToolProperty> = {};

  for (const [key, value] of Object.entries(tool.parameters.properties)) {
    const { description: _desc, ...rest } = value;
    cleanedProperties[key] = rest as ToolProperty;
  }

  return {
    ...tool,
    parameters: {
      ...tool.parameters,
      properties: cleanedProperties,
    },
  };
}