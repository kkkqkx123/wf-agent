/**
 * Tool Format Selector
 *
 * Provides utilities for selecting appropriate prompt templates and parser options
 * based on the configured tool call format.
 */

import type { ToolCallFormat, ToolCallFormatConfig, ToolCallFormatMarkers } from "@wf-agent/types";
import { DEFAULT_JSON_MARKERS, migrateToolMode, getDefaultFormatConfig } from "@wf-agent/types";
import type { PromptTemplate } from "@wf-agent/prompt-templates";
import {
  TOOLS_XML_LIST_TEMPLATE,
  TOOLS_JSON_LIST_TEMPLATE,
  TOOLS_RAW_LIST_TEMPLATE,
  TOOLS_RAW_COMPACT_LIST_TEMPLATE,
  TOOL_XML_FORMAT_TEMPLATE,
  TOOL_JSON_FORMAT_TEMPLATE,
  TOOL_RAW_FORMAT_TEMPLATE,
  TOOL_RAW_COMPACT_TEMPLATE,
  TOOL_XML_PARAMETER_LINE_TEMPLATE,
  TOOL_JSON_PARAMETER_LINE_TEMPLATE,
  TOOL_RAW_PARAMETER_LINE_TEMPLATE,
} from "@wf-agent/prompt-templates";
import type { ToolCallParseOptions } from "./types.js";

/**
 * Tool format template set
 * Contains all templates needed for a specific format
 */
export interface ToolFormatTemplateSet {
  /** List template for multiple tools */
  listTemplate: PromptTemplate;
  /** Single tool template */
  singleTemplate: PromptTemplate;
  /** Parameter line template */
  parameterTemplate: PromptTemplate;
}

/**
 * Get the appropriate template set for a tool call format
 *
 * @param format Tool call format
 * @param compact Whether to use compact templates (for limited context)
 * @returns Template set for the format
 */
export function getToolFormatTemplates(
  format: ToolCallFormat,
  compact: boolean = false,
): ToolFormatTemplateSet {
  switch (format) {
    case "function_call":
      // Native function call doesn't need special templates
      // Tools are passed via API's tools parameter
      return {
        listTemplate: TOOLS_RAW_LIST_TEMPLATE,
        singleTemplate: TOOL_RAW_FORMAT_TEMPLATE,
        parameterTemplate: TOOL_RAW_PARAMETER_LINE_TEMPLATE,
      };

    case "xml":
      return {
        listTemplate: TOOLS_XML_LIST_TEMPLATE,
        singleTemplate: TOOL_XML_FORMAT_TEMPLATE,
        parameterTemplate: TOOL_XML_PARAMETER_LINE_TEMPLATE,
      };

    case "json_wrapped":
      return {
        listTemplate: TOOLS_JSON_LIST_TEMPLATE,
        singleTemplate: TOOL_JSON_FORMAT_TEMPLATE,
        parameterTemplate: TOOL_JSON_PARAMETER_LINE_TEMPLATE,
      };

    case "json_raw":
      if (compact) {
        return {
          listTemplate: TOOLS_RAW_COMPACT_LIST_TEMPLATE,
          singleTemplate: TOOL_RAW_COMPACT_TEMPLATE,
          parameterTemplate: TOOL_RAW_PARAMETER_LINE_TEMPLATE,
        };
      }
      return {
        listTemplate: TOOLS_RAW_LIST_TEMPLATE,
        singleTemplate: TOOL_RAW_FORMAT_TEMPLATE,
        parameterTemplate: TOOL_RAW_PARAMETER_LINE_TEMPLATE,
      };

    default:
      // Default to raw JSON format
      return {
        listTemplate: TOOLS_RAW_LIST_TEMPLATE,
        singleTemplate: TOOL_RAW_FORMAT_TEMPLATE,
        parameterTemplate: TOOL_RAW_PARAMETER_LINE_TEMPLATE,
      };
  }
}

/**
 * Get tool call parser options for a specific format
 *
 * @param format Tool call format
 * @param customMarkers Optional custom markers for wrapped JSON
 * @returns Parser options
 */
export function getToolCallParserOptions(
  format: ToolCallFormat,
  customMarkers?: ToolCallFormatMarkers,
): ToolCallParseOptions {
  switch (format) {
    case "function_call":
      // Native format uses API's built-in parsing
      return {
        preferredFormats: ["raw"],
      };

    case "xml":
      return {
        preferredFormats: ["xml"],
      };

    case "json_wrapped":
      return {
        preferredFormats: ["json"],
        markers: customMarkers || DEFAULT_JSON_MARKERS,
      };

    case "json_raw":
      return {
        preferredFormats: ["raw"],
      };

    default:
      // Try all formats
      return {
        preferredFormats: ["xml", "json", "raw"],
      };
  }
}

/**
 * Resolve tool call format configuration
 * Applies defaults
 *
 * @param config Formatter configuration
 * @returns Resolved tool call format configuration
 */
export function resolveToolCallFormatConfig(
  config: Partial<{
    toolCallFormat?: ToolCallFormatConfig;
  }>,
): ToolCallFormatConfig {
  // If toolCallFormat is provided, use it directly
  if (config.toolCallFormat) {
    return {
      ...getDefaultFormatConfig(config.toolCallFormat.format),
      ...config.toolCallFormat,
    };
  }

  // Default to function_call
  return getDefaultFormatConfig("function_call");
}

/**
 * Check if a format requires tool descriptions in the prompt
 *
 * @param format Tool call format
 * @returns Whether tool descriptions should be included in prompt
 */
export function requiresPromptToolDescriptions(format: ToolCallFormat): boolean {
  return format !== "function_call";
}

/**
 * Check if a format requires custom tool call parsing
 *
 * @param format Tool call format
 * @returns Whether custom parsing is needed
 */
export function requiresCustomParsing(format: ToolCallFormat): boolean {
  return format !== "function_call";
}

/**
 * Validate that profile format is compatible with prompt format
 *
 * @param profileFormat Format configured in profile
 * @param promptFormat Format used in prompt
 * @returns Validation result
 */
export function validateToolFormatCompatibility(
  profileFormat: ToolCallFormat,
  promptFormat: ToolCallFormat,
): { compatible: boolean; reason?: string } {
  if (profileFormat === promptFormat) {
    return { compatible: true };
  }

  // Special case: json_wrapped and json_raw are somewhat compatible
  // Both use JSON, just with different markers
  if (
    (profileFormat === "json_wrapped" && promptFormat === "json_raw") ||
    (profileFormat === "json_raw" && promptFormat === "json_wrapped")
  ) {
    return {
      compatible: true,
      reason: `Format mismatch: profile uses "${profileFormat}" but prompt uses "${promptFormat}". Both are JSON-based and may work, but markers may differ.`,
    };
  }

  return {
    compatible: false,
    reason: `Format mismatch: profile is configured for "${profileFormat}" but prompt expects "${promptFormat}". This will cause tool call parsing to fail.`,
  };
}

/**
 * Get format display name
 *
 * @param format Tool call format
 * @returns Human-readable format name
 */
export function getToolFormatDisplayName(format: ToolCallFormat): string {
  const names: Record<ToolCallFormat, string> = {
    function_call: "Native Function Call",
    xml: "XML Format",
    json_wrapped: "Wrapped JSON Format",
    json_raw: "Raw JSON Format",
  };
  return names[format] || format;
}

/**
 * Get format description
 *
 * @param format Tool call format
 * @returns Format description
 */
export function getToolFormatDescription(format: ToolCallFormat): string {
  const descriptions: Record<ToolCallFormat, string> = {
    function_call: "Uses the LLM provider's native function calling API (OpenAI, Anthropic, etc.)",
    xml: "Tools described in XML format, LLM outputs XML tool calls",
    json_wrapped: "Tools described in JSON, LLM outputs JSON wrapped with custom markers",
    json_raw: "Tools described in JSON, LLM outputs raw JSON without markers",
  };
  return descriptions[format] || `Unknown format: ${format}`;
}

/**
 * Get all available tool call formats
 *
 * @returns Array of format information
 */
export function getAvailableToolFormats(): Array<{
  value: ToolCallFormat;
  name: string;
  description: string;
}> {
  const formats: ToolCallFormat[] = ["function_call", "xml", "json_wrapped", "json_raw"];

  return formats.map(format => ({
    value: format,
    name: getToolFormatDisplayName(format),
    description: getToolFormatDescription(format),
  }));
}
