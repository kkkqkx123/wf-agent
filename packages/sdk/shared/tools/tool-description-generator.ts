/**
 * Tool Description Generator
 *
 * Generates standardized tool descriptions for LLM consumption.
 * Supports multiple description formats including XML, JSON Schema, and plain text.
 *
 * @remarks
 * The tool description generator provides a unified interface for generating
 * tool descriptions in various formats suitable for different LLM providers.
 * It handles the conversion of tool definitions into human-readable and
 * LLM-parseable descriptions.
 */

import { renderToolDescription } from "../../resources/predefined/prompt-templates/tool-description-renderer.js";
import { convertToolParameters } from "./tool-parameter-converter.js";
import type { Tool, ToolDescriptionData } from "@wf-agent/types";

/**
 * Generate a tool description for a given tool
 *
 * @param tool - The tool object to generate a description for
 * @returns A string containing the tool description
 */
export function generateToolDescription(tool: Tool): string {
  const { id, description, parameters } = tool;

  const convertedParameters = parameters ? convertToolParameters(parameters) : undefined;

  // Convert Record<string, ConvertedParameter> to ToolParameterDescription[]
  const paramArray: ToolParameterDescription[] = convertedParameters
    ? Object.entries(convertedParameters).map(([name, param]) => ({
        name,
        type: param.type || "string",
        required: param.required,
        description: param.description || "",
      }))
    : [];

  return renderToolDescription({
    id,
    description: description || "No description",
    parameters: paramArray,
  } as unknown as ToolDescriptionData);
}

/**
 * Generate a brief tool description (name only with description)
 *
 * @param tool - The tool object
 * @returns A brief description string
 */
export function generateBriefToolDescription(tool: Tool): string {
  const { id, description } = tool;
  return `${id}: ${description || "No description"}`;
}

/**
 * Generate a tool list summary
 *
 * @param tools - The array of tools to summarize
 * @returns A summary string of all tools
 */
export function generateToolListSummary(tools: Tool[]): string {
  if (tools.length === 0) {
    return "No tools available";
  }

  return tools
    .map(tool => {
      const { id, description } = tool;
      return `${id}: ${description || "No description"}`;
    })
    .join("\n");
}

/**
 * Generate descriptions for a list of tools
 *
 * @param tools - The array of tools to generate descriptions for
 * @returns An array of tool description strings
 */
export function generateToolDescriptions(tools: Tool[]): string[] {
  return tools.map(tool => generateToolDescription(tool));
}

/**
 * Tool description format types
 */
export type ToolDescriptionFormat = "detailed" | "compact" | "minimal";

/**
 * Generate a tool list description
 * @param tools - The tools to describe
 * @param format - The output format
 * @param options - Additional options
 * @returns A formatted string describing the tools
 */
export function generateToolListDescription(
  tools: Tool[],
  format: ToolDescriptionFormat = "detailed",
  options?: { groupByCategory?: boolean },
): string {
  // Delegate to existing functions
  if (format === "minimal") {
    return tools.map(t => `${t.id}: ${t.description || "No description"}`).join("\n");
  }
  const descriptions = generateToolDescriptions(tools);
  if (options?.groupByCategory) {
    return descriptions.join("\n---\n");
  }
  return descriptions.join("\n");
}

/**
 * Generate a tool availability section
 * @param tools - The tools to describe
 * @param format - The output format
 * @returns A formatted string describing tool availability
 */
export function generateToolAvailabilitySection(
  tools: Tool[],
  format: ToolDescriptionFormat = "detailed",
): string {
  return generateToolListDescription(tools, format);
}