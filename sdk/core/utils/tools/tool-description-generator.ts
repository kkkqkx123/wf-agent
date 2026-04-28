/**
 * Tool Description Generator
 *
 * Provides tool description generation that leverages ToolDescriptionData
 * from the tool description registry when available.
 *
 * Features:
 * - Uses predefined ToolDescriptionData for rich tool descriptions
 * - Falls back to converting Tool.parameters (JSON Schema) when needed
 * - Supports multiple output formats: default, single-line, list, table, detailed, compact
 * - Generates complete tool documentation including parameters, tips, and examples
 */

import type { Tool } from "@wf-agent/types";
import type { ToolDescriptionData, ToolParameterDescription } from "@wf-agent/prompt-templates";
import {
  renderToolDescription,
  renderToolDescriptionSingleLine,
  renderToolDescriptionListItem,
  renderToolDescriptionTableRow,
} from "@wf-agent/prompt-templates";
import { toolDescriptionRegistry } from "./tool-description-registry.js";
import { convertToolParameters } from "./tool-parameter-converter.js";

/**
 * Tool description format type
 */
export type ToolDescriptionFormat =
  | "default"
  | "single-line"
  | "list"
  | "table"
  | "detailed"
  | "compact";

/**
 * Tool with description data
 * Combines runtime Tool with its description data
 */
export interface ToolWithDescription {
  tool: Tool;
  descriptionData?: ToolDescriptionData;
}

/**
 * Get or create ToolDescriptionData for a tool
 *
 * Priority:
 * 1. Look up in toolDescriptionRegistry by tool ID
 * 2. Convert Tool.parameters (JSON Schema) to ToolDescriptionData
 *
 * @param tool Tool object
 * @returns ToolDescriptionData (either from registry or converted)
 */
export function getToolDescriptionData(tool: Tool): ToolDescriptionData {
  // 1. Try to get from registry
  const registered = toolDescriptionRegistry.get(tool.id);
  if (registered) {
    return registered;
  }

  // 2. Convert from Tool (JSON Schema parameters)
  return convertToolToDescriptionData(tool);
}

/**
 * Convert Tool to ToolDescriptionData
 *
 * @param tool Tool object
 * @returns Converted ToolDescriptionData
 */
export function convertToolToDescriptionData(tool: Tool): ToolDescriptionData {
  const parameters = tool.parameters ? convertToolParameters(tool.parameters) : [];

  return {
    name: tool.name,
    id: tool.id,
    type: tool.type === "STATEFUL" ? "STATEFUL" : "STATELESS",
    category: (tool.metadata?.category as ToolDescriptionData["category"]) || undefined,
    description: tool.description || "No description available",
    parameters,
  };
}

/**
 * Generate tool description
 *
 * @param tool Tool object
 * @param format Output format
 * @returns Formatted tool description string
 */
export function generateToolDescription(
  tool: Tool,
  format: ToolDescriptionFormat = "default",
): string {
  const descriptionData = getToolDescriptionData(tool);

  switch (format) {
    case "detailed":
      return renderToolDescription(descriptionData);

    case "compact":
      return generateCompactDescription(descriptionData);

    case "table":
      return renderToolDescriptionTableRow(descriptionData);

    case "list":
      return renderToolDescriptionListItem(descriptionData);

    case "single-line":
      return renderToolDescriptionSingleLine(descriptionData);

    case "default":
    default:
      // Default format: include parameters but not tips/examples
      return generateDefaultDescription(descriptionData);
  }
}

/**
 * Generate default format description (includes parameters)
 */
function generateDefaultDescription(data: ToolDescriptionData): string {
  const parts: string[] = [
    `${data.name}: ${data.description}`,
    "",
    "Parameters:",
    renderParameters(data.parameters),
  ];

  return parts.join("\n");
}

/**
 * Generate compact format description (minimal)
 */
function generateCompactDescription(data: ToolDescriptionData): string {
  const paramSummary = data.parameters.length > 0 ? ` (${data.parameters.length} params)` : "";
  return `${data.name}${paramSummary}: ${data.description}`;
}

/**
 * Render parameters
 */
function renderParameters(parameters: ToolParameterDescription[]): string {
  if (parameters.length === 0) {
    return "  None";
  }

  return parameters
    .map(param => {
      const req = param.required ? "required" : "optional";
      const def = param.defaultValue !== undefined ? `, default: ${param.defaultValue}` : "";
      return `  - ${param.name} (${param.type}, ${req}${def}): ${param.description}`;
    })
    .join("\n");
}

/**
 * Generate tool list description
 *
 * @param tools Array of tools
 * @param format Output format
 * @param options Optional configuration
 * @returns Formatted tool list description
 */
export function generateToolListDescription(
  tools: Tool[],
  format: ToolDescriptionFormat = "default",
  options?: {
    /** Whether to include headers (for table format) */
    includeHeader?: boolean;
    /** Custom separator (for single-line format) */
    separator?: string;
    /** Group by category */
    groupByCategory?: boolean;
  },
): string {
  if (!tools || tools.length === 0) {
    return "";
  }

  // Group by category if requested
  if (options?.groupByCategory && format !== "table") {
    return generateGroupedDescription(tools, format);
  }

  const descriptions = tools.map(tool => generateToolDescription(tool, format));

  switch (format) {
    case "table":
      // Table format: One tool description per row
      if (options?.includeHeader) {
        const header =
          "| Tool Name | Tool ID | Description |\n|-----------|---------|-------------|";
        return `${header}\n${descriptions.join("\n")}`;
      }
      return descriptions.join("\n");

    case "single-line": {
      // Single-line format: Connect using delimiters
      const separator = options?.separator || "\n";
      return descriptions.join(separator);
    }

    case "list":
      // List format: One tool description per line
      return descriptions.join("\n");

    case "default":
    case "detailed":
    case "compact":
    default:
      // Add separator between tools for multi-line formats
      return descriptions.join("\n\n");
  }
}

/**
 * Generate category-grouped description
 */
function generateGroupedDescription(tools: Tool[], format: ToolDescriptionFormat): string {
  const byCategory = new Map<string, Tool[]>();
  const uncategorized: Tool[] = [];

  for (const tool of tools) {
    const descriptionData = getToolDescriptionData(tool);
    const category = descriptionData.category;

    if (category) {
      const list = byCategory.get(category) || [];
      list.push(tool);
      byCategory.set(category, list);
    } else {
      uncategorized.push(tool);
    }
  }

  const sections: string[] = [];

  // Add categorized tools
  const sortedCategories = Array.from(byCategory.keys()).sort();
  for (const category of sortedCategories) {
    const categoryTools = byCategory.get(category)!;
    const descriptions = categoryTools.map(t => generateToolDescription(t, format));

    sections.push(`## ${capitalizeFirst(category)} Tools\n\n${descriptions.join("\n\n")}`);
  }

  // Add uncategorized tools
  if (uncategorized.length > 0) {
    const descriptions = uncategorized.map(t => generateToolDescription(t, format));
    sections.push(`## Other Tools\n\n${descriptions.join("\n\n")}`);
  }

  return sections.join("\n\n");
}

/**
 * Capitalize first letter
 */
function capitalizeFirst(str: string): string {
  return str.charAt(0).toUpperCase() + str.slice(1);
}

/**
 * Generate tool availability section for system prompts
 *
 * This is the main entry point for building tool descriptions in system prompts.
 *
 * @param tools Available tools
 * @param format Output format
 * @returns Complete tool availability description
 */
export function generateToolAvailabilitySection(
  tools: Tool[],
  format: ToolDescriptionFormat = "detailed",
): string {
  if (!tools || tools.length === 0) {
    return "No tools available.";
  }

  const parts: string[] = [
    `You have access to ${tools.length} tool${tools.length === 1 ? "" : "s"}:",`,
    "",
    generateToolListDescription(tools, format, { groupByCategory: true }),
    "",
    "### Tool Usage Rules",
    "1. Only use the tools listed above",
    "2. Follow the exact parameter schema for each tool",
    "3. Wait for tool execution results before making the next call",
    "4. Use the correct tool call format (XML or JSON) as specified",
  ];

  return parts.join("\n");
}

/**
 * Generate tool table row (for tool visibility declaration)
 *
 * @param tool Tool object
 * @returns Table row string
 */
export function generateToolTableRow(tool: Tool): string {
  return generateToolDescription(tool, "table");
}

/**
 * Generate tool table (with header)
 *
 * @param tools Array of tools
 * @returns Complete table string
 */
export function generateToolTable(tools: Tool[]): string {
  return generateToolListDescription(tools, "table", { includeHeader: true });
}
