/**
 * Tool Description Render Template
 *
 * Tool function to render ToolDescriptionData as text
 * Supports multiple output formats: default, single line, table.
 */

import type {
  ToolDescriptionData,
  ToolParameterDescription,
} from "../../types/tool-description.js";

/**
 * Description of rendering parameters
 */
function renderParameter(param: ToolParameterDescription): string {
  const required = param.required ? "required" : "optional";
  const defaultValue = param.defaultValue !== undefined ? `, default: ${param.defaultValue}` : "";
  return `  - ${param.name} (${param.type}, ${required}${defaultValue}): ${param.description}`;
}

/**
 * Render parameter list
 */
function renderParameters(parameters: ToolParameterDescription[]): string {
  if (parameters.length === 0) {
    return "  None";
  }
  return parameters.map(renderParameter).join("\n");
}

/**
 * Render cue list
 */
function renderTips(tips: string[] | undefined): string {
  if (!tips || tips.length === 0) {
    return "";
  }
  const tipLines = tips.map(tip => `  - ${tip}`).join("\n");
  return `\nTips:\n${tipLines}`;
}

/**
 * List of Rendering Examples
 */
function renderExamples(examples: string[] | undefined): string {
  if (!examples || examples.length === 0) {
    return "";
  }
  const exampleLines = examples.map(example => `  - ${example}`).join("\n");
  return `\nExamples:\n${exampleLines}`;
}

/**
 * Render tool description data as full text (default format)
 */
export function renderToolDescription(data: ToolDescriptionData): string {
  const parts: string[] = [
    data.description,
    "",
    "Parameters:",
    renderParameters(data.parameters),
    renderTips(data.tips),
    renderExamples(data.examples),
  ];

  return parts.filter(part => part !== "").join("\n");
}

/**
 * Renders the tool description into a single line format
 */
export function renderToolDescriptionSingleLine(data: ToolDescriptionData): string {
  return `${data.name}: ${data.description}`;
}

/**
 * Renders the tool description into a list item format
 */
export function renderToolDescriptionListItem(data: ToolDescriptionData): string {
  return `- ${data.name}: ${data.description}`;
}

/**
 * Rendering tool descriptions into table row format
 */
export function renderToolDescriptionTableRow(data: ToolDescriptionData): string {
  return `| ${data.name} | ${data.id} | ${data.description} |`;
}

/**
 * Batch Render Multiple Tool Descriptions
 */
export function renderToolDescriptions(
  tools: ToolDescriptionData[],
  format: "default" | "single-line" | "list" | "table" = "default",
): string {
  switch (format) {
    case "single-line":
      return tools.map(renderToolDescriptionSingleLine).join("\n");
    case "list":
      return tools.map(renderToolDescriptionListItem).join("\n");
    case "table":
      return tools.map(renderToolDescriptionTableRow).join("\n");
    case "default":
    default:
      return tools.map(renderToolDescription).join("\n\n");
  }
}
