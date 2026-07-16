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
  | "native" // Native API function calling (OpenAI, Anthropic, etc.)
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
 *
 * Defines the XML tag names used for tool call and tool result formatting.
 * All tags are required except `item` (used for array items in tool parameters).
 */
export interface ToolCallXmlTags {
  /** Root tag for a tool call invocation (e.g. "tool_use") */
  toolCall: string;
  /** Tag for tool name (e.g. "tool_name") */
  toolName: string;
  /** Tag for tool arguments/parameters (e.g. "parameters") */
  toolArgs: string;
  /** Root tag for a tool result (e.g. "tool_result") */
  toolResult: string;
  /** Tag for tool call ID in result (e.g. "tool_call_id") */
  toolCallId: string;
  /** Tag for tool output/content (e.g. "tool_output") */
  toolOutput: string;
  /** Tag for array items in parameters (e.g. "item") */
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

  /** Whether to include tool description in prompt (default: true) */
  includeDescription?: boolean;

  /** Tool description format style (default: "detailed") */
  descriptionStyle?: "detailed" | "compact" | "minimal";

  /** Whether to include tool usage examples in prompt */
  includeExamples?: boolean;

  /** Whether to include tool usage rules in prompt */
  includeRules?: boolean;
}

/**
 * Default tool call format configuration
 */
export const DEFAULT_TOOL_CALL_FORMAT_CONFIG: ToolCallFormatConfig = {
  format: "native",
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
  toolCall: "tool_use",
  toolName: "tool_name",
  toolArgs: "parameters",
  toolResult: "tool_result",
  toolCallId: "tool_call_id",
  toolOutput: "tool_output",
  item: "item",
};

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
  "native",
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
  toolCall: z.string().min(1, { message: "Tool call tag is required" }),
  toolName: z.string().min(1, { message: "Tool name tag is required" }),
  toolArgs: z.string().min(1, { message: "Tool args tag is required" }),
  toolResult: z.string().min(1, { message: "Tool result tag is required" }),
  toolCallId: z.string().min(1, { message: "Tool call ID tag is required" }),
  toolOutput: z.string().min(1, { message: "Tool output tag is required" }),
  item: z.string().optional(),
});

/**
 * Tool Call Format Config Schema
 */
export const ToolCallFormatConfigSchema: z.ZodType<ToolCallFormatConfig> = z.object({
  format: ToolCallFormatSchema,
  markers: ToolCallFormatMarkersSchema.optional(),
  xmlTags: ToolCallXmlTagsSchema.optional(),
  includeDescription: z.boolean().optional(),
  descriptionStyle: z.enum(["detailed", "compact", "minimal"]).optional(),
  includeExamples: z.boolean().optional(),
  includeRules: z.boolean().optional(),
});
