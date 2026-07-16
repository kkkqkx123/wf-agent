/**
 * Tool Format Selector
 *
 * Provides utilities for selecting appropriate prompt templates and parser options
 * based on the configured tool call format.
 */

import type { ToolCallFormat, ToolCallFormatConfig, ToolCallFormatMarkers, ToolCallProtocolViolationPolicy } from "@wf-agent/types";
import { DEFAULT_JSON_MARKERS, getDefaultFormatConfig, validateToolCallFormatConfig } from "@wf-agent/types";
import type { PromptTemplate } from "@wf-agent/types";
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
} from "../../../resources/predefined/prompt-templates/tool-format-templates.js";
import type { ToolCallParseOptions } from "./types.js";
import { sdkLogger as logger } from "../../../utils/logger.js";

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
    case "native":
      // Native function call: tools are passed via API's tools parameter,
      // but we still include raw-style templates as a fallback for
      // text-mode scenarios (e.g. prompt injection for non-native providers).
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
    case "native":
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
    const resolved: ToolCallFormatConfig = {
      ...getDefaultFormatConfig(config.toolCallFormat.format),
      ...config.toolCallFormat,
    };

    // Validate the resolved configuration
    const validation = validateToolCallFormatConfig(resolved);
    if (!validation.valid) {
      logger.warn("Invalid tool call format configuration", {
        errors: validation.errors,
        config: config.toolCallFormat,
      });
    }

    return resolved;
  }

  // Default to native
  return getDefaultFormatConfig("native");
}

/**
 * Check if a format requires tool descriptions in the prompt
 *
 * @param format Tool call format
 * @returns Whether tool descriptions should be included in prompt
 */
export function requiresPromptToolDescriptions(format: ToolCallFormat): boolean {
  return format !== "native";
}

/**
 * Check if a format requires custom tool call parsing
 *
 * @param format Tool call format
 * @returns Whether custom parsing is needed
 */
export function requiresCustomParsing(format: ToolCallFormat): boolean {
  return format !== "native";
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
    native: "Native Function Call",
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
    native: "Uses the LLM provider's native function calling API (OpenAI, Anthropic, etc.)",
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
  const formats: ToolCallFormat[] = ["native", "xml", "json_wrapped", "json_raw"];

  return formats.map(format => ({
    value: format,
    name: getToolFormatDisplayName(format),
    description: getToolFormatDescription(format),
  }));
}

/**
 * Context for a protocol violation event.
 */
export interface ProtocolViolationContext {
  /** The locked format that should be used */
  lockedFormat: ToolCallFormatConfig;
  /** The format that was attempted (from profile or request) */
  attemptedFormat?: ToolCallFormatConfig;
  /** Execution ID for tracing */
  executionId: string;
  /** Entity ID (optional) */
  entityId?: string;
  /** Profile ID that triggered the violation */
  profileId: string;
  /** Current iteration number (optional) */
  iteration?: number;
}

/**
 * Error thrown when a protocol violation is detected with the "fail" policy.
 */
export class ProtocolViolationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ProtocolViolationError";
  }
}

/**
 * Handle a tool call protocol violation.
 *
 * Called when the locked tool call format differs from the format
 * requested by the current LLM profile or request.
 *
 * @param context - Context information about the violation
 * @param policy - The policy to apply
 * @throws {ProtocolViolationError} When policy is "fail"
 */
export function handleProtocolViolation(
  context: ProtocolViolationContext,
  policy: ToolCallProtocolViolationPolicy,
): void {
  switch (policy) {
    case "ignore":
      // Silently use the locked protocol, no logging
      return;

    case "warn":
      logger.warn("Tool call protocol violation detected", {
        lockedFormat: context.lockedFormat.format,
        attemptedFormat: context.attemptedFormat?.format,
        executionId: context.executionId,
        entityId: context.entityId,
        profileId: context.profileId,
        iteration: context.iteration,
      });
      // Continue with locked protocol
      return;

    case "fail":
      logger.error("Tool call protocol violation \u2014 interrupting execution", {
        lockedFormat: context.lockedFormat.format,
        attemptedFormat: context.attemptedFormat?.format,
        executionId: context.executionId,
      });
      throw new ProtocolViolationError(
        `Tool call protocol conflict: locked "${context.lockedFormat.format}" ` +
        `but profile "${context.profileId}" attempted "${context.attemptedFormat?.format}". ` +
        "Execution interrupted per fail policy.",
      );

    case "auto_convert":
      logger.info("Auto-converting tool call protocol", {
        from: context.attemptedFormat?.format,
        to: context.lockedFormat.format,
        executionId: context.executionId,
      });
      // The locked format is already enforced by the formatter.
      // HistoryConverter will handle the conversion on the next request.
      return;
  }
}
