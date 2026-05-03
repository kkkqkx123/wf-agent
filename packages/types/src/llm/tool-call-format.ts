/**
 * Tool Call Format Type Definitions
 *
 * Defines types for tool calling formats, including how tools are described
 * in prompts and how LLMs should format their tool calls.
 */

import { z } from "zod";

/**
 * Tool call format types
 * Defines how tools are described in prompts and how LLM should format tool calls
 */
export type ToolCallFormat =
  | "function_call" // Native API function calling (OpenAI, Anthropic, etc.)
  | "xml" // XML format for non-function-calling models
  | "json_wrapped" // JSON wrapped with custom markers (e.g., <<<TOOL_CALL>>>)
  | "json_raw"; // Raw JSON without markers

/**
 * Custom markers for wrapped JSON format
 */
export interface ToolCallFormatMarkers {
  /** Start marker for tool call */
  start: string;
  /** End marker for tool call */
  end: string;
}

/**
 * XML tag configuration for XML format
 */
export interface ToolCallXmlTags {
  /** Root tag for tool use */
  toolUse: string;
  /** Tag for tool name */
  toolName: string;
  /** Tag for parameters section */
  parameters: string;
  /** Tag for array items */
  item?: string;
}

/**
 * Tool call format configuration
 * Provides detailed control over tool calling behavior
 */
export interface ToolCallFormatConfig {
  /** Format type */
  format: ToolCallFormat;

  /** Custom markers for wrapped JSON format (only used when format is "json_wrapped") */
  markers?: ToolCallFormatMarkers;

  /** XML tag names for XML format (only used when format is "xml") */
  xmlTags?: ToolCallXmlTags;

  /** Whether to include tool description in prompt */
  includeDescription: boolean;

  /** Tool description format style */
  descriptionStyle: "detailed" | "compact" | "minimal";

  /** Whether to include tool usage examples in prompt */
  includeExamples?: boolean;

  /** Whether to include tool usage rules in prompt */
  includeRules?: boolean;
}

/**
 * Default tool call format configuration
 */
export const DEFAULT_TOOL_CALL_FORMAT_CONFIG: ToolCallFormatConfig = {
  format: "function_call",
  includeDescription: true,
  descriptionStyle: "detailed",
  includeExamples: true,
  includeRules: true,
};

/**
 * Default markers for wrapped JSON format
 */
export const DEFAULT_JSON_MARKERS: ToolCallFormatMarkers = {
  start: "<<<TOOL_CALL>>>",
  end: "<<<END_TOOL_CALL>>>",
};

/**
 * Default XML tags for XML format
 */
export const DEFAULT_XML_TAGS: ToolCallXmlTags = {
  toolUse: "tool_use",
  toolName: "tool_name",
  parameters: "parameters",
  item: "item",
};

/**
 * Migration helper: Convert legacy toolMode to ToolCallFormatConfig
 * @param toolMode Legacy tool mode string
 * @returns ToolCallFormatConfig or undefined if toolMode is not provided
 */
export function migrateToolMode(toolMode?: string): ToolCallFormatConfig | undefined {
  if (!toolMode) return undefined;

  const formatMap: Record<string, ToolCallFormat> = {
    function_call: "function_call",
    xml: "xml",
    json: "json_wrapped",
    raw: "json_raw",
  };

  const format = formatMap[toolMode];
  if (!format) return undefined;

  return {
    format,
    markers: format === "json_wrapped" ? DEFAULT_JSON_MARKERS : undefined,
    xmlTags: format === "xml" ? DEFAULT_XML_TAGS : undefined,
    includeDescription: true,
    descriptionStyle: "detailed",
    includeExamples: true,
    includeRules: true,
  };
}

/**
 * Get default format configuration for a specific format
 * @param format Tool call format
 * @returns Default configuration for the format
 */
export function getDefaultFormatConfig(format: ToolCallFormat): ToolCallFormatConfig {
  return {
    format,
    markers: format === "json_wrapped" ? DEFAULT_JSON_MARKERS : undefined,
    xmlTags: format === "xml" ? DEFAULT_XML_TAGS : undefined,
    includeDescription: true,
    descriptionStyle: "detailed",
    includeExamples: true,
    includeRules: true,
  };
}

/**
 * Validate tool call format configuration
 * @param config Configuration to validate
 * @returns Validation result
 */
export function validateToolCallFormatConfig(config: ToolCallFormatConfig): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!config.format) {
    errors.push("Format is required");
  }

  if (config.format === "json_wrapped" && !config.markers) {
    errors.push("Markers are required for json_wrapped format");
  }

  if (config.format === "xml" && !config.xmlTags) {
    errors.push("XML tags are required for xml format");
  }

  return {
    valid: errors.length === 0,
    errors,
  };
}

/**
 * Tool Call Format Schema
 */
export const ToolCallFormatSchema: z.ZodType<ToolCallFormat> = z.enum([
  "function_call",
  "xml",
  "json_wrapped",
  "json_raw",
]);

/**
 * Tool Call Format Markers Schema
 */
export const ToolCallFormatMarkersSchema: z.ZodType<ToolCallFormatMarkers> = z.object({
  start: z.string().min(1, { message: "Start marker is required" }),
  end: z.string().min(1, { message: "End marker is required" }),
});

/**
 * Tool Call XML Tags Schema
 */
export const ToolCallXmlTagsSchema: z.ZodType<ToolCallXmlTags> = z.object({
  toolUse: z.string().min(1, { message: "Tool use tag is required" }),
  toolName: z.string().min(1, { message: "Tool name tag is required" }),
  parameters: z.string().min(1, { message: "Parameters tag is required" }),
  item: z.string().optional(),
});

/**
 * Tool Call Format Config Schema
 */
export const ToolCallFormatConfigSchema: z.ZodType<ToolCallFormatConfig> = z.object({
  format: ToolCallFormatSchema,
  markers: ToolCallFormatMarkersSchema.optional(),
  xmlTags: ToolCallXmlTagsSchema.optional(),
  includeDescription: z.boolean(),
  descriptionStyle: z.enum(["detailed", "compact", "minimal"]),
  includeExamples: z.boolean().optional(),
  includeRules: z.boolean().optional(),
});
