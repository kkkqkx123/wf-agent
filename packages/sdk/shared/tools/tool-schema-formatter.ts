/**
 * Tool Schema Formatter
 *
 * Provides utilities for formatting tool schemas into various output formats.
 * Supports XML and JSON Schema output formats.
 *
 * @remarks
 * The tool schema formatter is used to convert tool schemas into different
 * output formats that are suitable for different LLM providers. It supports
 * both XML and JSON Schema formats, and provides a unified interface for
 * formatting tool schemas.
 */

import { renderTemplate } from "../utils/template-renderer/index.js";
import {
  TOOL_XML_FORMAT_TEMPLATE,
  TOOL_JSON_FORMAT_TEMPLATE,
} from "../../resources/predefined/prompt-templates/tool-format-templates.js";
import type { Tool } from "@wf-agent/types";

/**
 * Supported output formats
 */
export type ToolSchemaFormat = "xml" | "json-schema";

/**
 * Format a tool's schema into the specified format
 *
 * @param tool - The tool to format
 * @param format - The output format
 * @returns A formatted schema string
 */
export function formatToolSchema(tool: Tool, format: ToolSchemaFormat = "xml"): string {
  switch (format) {
    case "xml":
      return formatToolSchemaXml(tool);
    case "json-schema":
      return formatToolSchemaJson(tool);
    default:
      return formatToolSchemaXml(tool);
  }
}

/**
 * Format a tool schema in XML format
 *
 * @param tool - The tool to format
 * @returns An XML-formatted schema string
 */
function formatToolSchemaXml(tool: Tool): string {
  const { id, description, parameters } = tool;

  return renderTemplate(TOOL_XML_FORMAT_TEMPLATE.content, {
    toolId: id,
    toolDescription: description || "No description",
    parameters: parameters ? JSON.stringify(parameters, null, 2) : "No parameters",
  });
}

/**
 * Format a tool schema in JSON Schema format
 *
 * @param tool - The tool to format
 * @returns A JSON Schema-formatted string
 */
function formatToolSchemaJson(tool: Tool): string {
  const { id, description, parameters } = tool;

  return renderTemplate(TOOL_JSON_FORMAT_TEMPLATE.content, {
    toolId: id,
    toolDescription: description || "No description",
    parameters: parameters ? JSON.stringify(parameters, null, 2) : "No parameters",
  });
}

/**
 * Format a list of tool schemas into the specified format
 *
 * @param tools - The tools to format
 * @param format - The output format
 * @returns An array of formatted schema strings
 */
export function formatToolSchemas(
  tools: Tool[],
  format: ToolSchemaFormat = "xml",
): string[] {
  return tools.map(tool => formatToolSchema(tool, format));
}