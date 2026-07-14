/**
 * Tool Declaration Formatter
 *
 * Provides a utility function to format tool declarations into a standardized
 * format suitable for LLM consumption.
 *
 * @remarks
 * The tool declaration formatter is used to generate tool strings that include
 * metadata such as the tool's id, description, and parameters in a format
 * that can be easily consumed by LLMs during conversation.
 *
 * It supports:
 * - Generating tool descriptions with and without parameter descriptions
 * - Including only the tool name list
 * - Handling tools with no parameters
 */

import type { Tool } from "@wf-agent/types";
import type { ToolCallFormatMarkers } from "@wf-agent/types";
import { generateToolParametersDescription } from "./tool-parameters-describer.js";

/**
 * Configuration for formatting tool declarations
 */
export interface ToolDeclarationFormatOptions {
  /** Whether to include detailed parameter descriptions (default: true) */
  includeParameters?: boolean;
  /** Whether to include the tool name-only list (default: false) */
  includeNameOnly?: boolean;
}

/**
 * Format a single tool's declaration string
 *
 * @param tool - The tool to format
 * @param options - Formatting options
 * @returns The formatted tool declaration string
 */
export function formatToolDeclaration(
  tool: Tool,
  options: ToolDeclarationFormatOptions = {},
): string {
  const { includeParameters = true } = options;

  if (!includeParameters) {
    return formatToolWithoutParameters(tool);
  }

  return generateToolParametersDescription(tool, { includeToolName: true });
}

/**
 * Format a tool declaration without parameters
 * @param tool - The tool to format
 * @returns A simple tool declaration string
 */
function formatToolWithoutParameters(tool: Tool): string {
  const description = tool.description || "No description";
  return `Tool: ${tool.id}\nDescription: ${description}`;
}

/**
 * Format a list of tools into a set of declarations
 *
 * @param tools - The tools to format
 * @param options - Formatting options
 * @returns An array of formatted tool declaration strings
 */
export function formatToolDeclarations(
  tools: Tool[],
  options: ToolDeclarationFormatOptions = {},
): string[] {
  return tools.map(tool => formatToolDeclaration(tool, options));
}

/**
 * Generate a name-only list of tools
 *
 * @param tools - The tools to list
 * @returns A formatted string listing tool names
 */
export function formatToolNameList(tools: Tool[]): string {
  if (tools.length === 0) {
    return "No tools are available";
  }

  return `AVAILABLE TOOLS:\n${tools.map(tool => `- ${tool.id}: ${tool.description || "No description"}`).join("\n")}`;
}

/**
 * Tool Declaration Formatter class
 * Provides static methods for formatting tool calls and results in XML/JSON formats.
 */
export class ToolDeclarationFormatter {
  /**
   * Format tool calls into a text representation
   */
  static formatToolCalls(
    toolCalls: Array<{ id: string; function: { name: string; arguments: string } }>,
    options: { format: "xml" | "json"; xmlTags: Record<string, string>; markers: ToolCallFormatMarkers },
  ): string {
    if (options.format === "xml") {
      const { xmlTags } = options;
      return toolCalls
        .map(tc => {
          const args = tc.function.arguments;
          return `<${xmlTags["toolCallTag"]}>\n<${xmlTags["toolNameTag"]}>${tc.function.name}</${xmlTags["toolNameTag"]}>\n<${xmlTags["toolArgsTag"]}>${args}</${xmlTags["toolArgsTag"]}>\n</${xmlTags["toolCallTag"]}>`;
        })
        .join("\n");
    }
    // JSON format
    return toolCalls
      .map(tc => {
        let parsedArgs: unknown;
        try {
          parsedArgs = JSON.parse(tc.function.arguments || "{}");
        } catch {
          parsedArgs = tc.function.arguments;
        }
        return `${options.markers.start}\n${JSON.stringify({ tool: tc.function.name, parameters: parsedArgs })}\n${options.markers.end}`;
      })
      .join("\n");
  }

  /**
   * Format a tool result message into a text representation
   */
  /**
   * Format a list of tools into declarations
   * This is an alias for formatToolDeclarations for compatibility
   */
  static formatTools(tools: Tool[]): string[] {
    return formatToolDeclarations(tools);
  }

  static formatToolResult(
    message: { toolCallId?: string; content: string },
    options: { format: "xml" | "json"; xmlTags: Record<string, string>; markers: ToolCallFormatMarkers },
  ): string {
    if (options.format === "xml") {
      const { xmlTags } = options;
      return `<${xmlTags["toolResultTag"]}>\n<${xmlTags["toolIdTag"]}>${message.toolCallId}</${xmlTags["toolIdTag"]}>\n<${xmlTags["toolOutputTag"]}>${message.content}</${xmlTags["toolOutputTag"]}>\n</${xmlTags["toolResultTag"]}>`;
    }
    return `${options.markers.start}\n${JSON.stringify({ tool_call_id: message.toolCallId, output: message.content })}\n${options.markers.end}`;
  }
}