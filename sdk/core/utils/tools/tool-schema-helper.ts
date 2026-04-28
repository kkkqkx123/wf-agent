/**
 * Tool Schema Helper
 *
 * Provides utility functions for preparing tool schemas for LLM calls.
 * Shared by both Agent and Graph modules.
 *
 * Design Principles:
 * - Stateless helper functions
 * - Reusable across Agent and Graph executors
 * - Encapsulates tool schema conversion logic
 */

import type { ToolRegistry } from "../../registry/tool-registry.js";
import type { ToolSchema, ToolParameterSchema } from "@wf-agent/types";

/**
 * Prepare tool schemas from tool IDs
 *
 * Converts tool IDs to LLM-compatible tool schemas by looking up
 * tool definitions from ToolRegistry.
 *
 * @param toolIds Array of tool IDs
 * @param toolService Tool service to lookup tool definitions
 * @returns Array of tool schemas, or undefined if no tools
 *
 * @example
 * ```ts
 * const schemas = prepareToolSchemas(['calculator', 'weather'], toolService);
 * // Returns: [{ id: 'calculator', description: '...', parameters: {...} }, ...]
 * ```
 */
export function prepareToolSchemas(
  toolIds: string[] | undefined,
  toolService: ToolRegistry,
): ToolSchema[] | undefined {
  if (!toolIds || toolIds.length === 0) {
    return undefined;
  }

  const schemas = toolIds
    .map(id => {
      try {
        const tool = toolService.getTool(id);
        return tool
          ? {
              id: tool.id,
              description: tool.description,
              parameters: tool.parameters as unknown,
            }
          : null;
      } catch {
        // Tool not found, skip
        return null;
      }
    })
    .filter((t): t is ToolSchema => t !== null);

  return schemas.length > 0 ? schemas : undefined;
}

/**
 * Prepare tool schemas from tool objects
 *
 * Converts tool objects to LLM-compatible tool schemas.
 * Used when tool objects are already available.
 *
 * @param tools Array of tool objects
 * @returns Array of tool schemas, or undefined if no tools
 */
export function prepareToolSchemasFromTools(
  tools: Array<{ id: string; description: string; parameters: unknown }> | undefined,
): ToolSchema[] | undefined {
  if (!tools || tools.length === 0) {
    return undefined;
  }

  return tools.map(tool => ({
    id: tool.id,
    description: tool.description,
    parameters: tool.parameters as ToolParameterSchema,
  }));
}

/**
 * Convert tool calls to executor format
 *
 * Converts LLM response tool calls to the format expected by ToolCallExecutor.
 *
 * @param toolCalls Tool calls from LLM response
 * @returns Array of tool calls in executor format
 */
export function convertToolCallsForExecutor(
  toolCalls: Array<{ id: string; name: string; arguments: string }>,
): Array<{ id: string; name: string; arguments: string }> {
  return toolCalls.map(tc => ({
    id: tc.id,
    name: tc.name,
    arguments: tc.arguments,
  }));
}

/**
 * Convert tool calls from LLM function format to executor format
 *
 * @param toolCalls Tool calls in LLM function format
 * @returns Array of tool calls in executor format
 */
export function convertLLMToolCallsForExecutor(
  toolCalls: Array<{
    id: string;
    function: { name: string; arguments: string };
  }>,
): Array<{ id: string; name: string; arguments: string }> {
  return toolCalls.map(tc => ({
    id: tc.id,
    name: tc.function.name,
    arguments: tc.function.arguments,
  }));
}
