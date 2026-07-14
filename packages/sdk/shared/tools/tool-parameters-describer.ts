/**
 * Tool Parameters Describer
 *
 * Generates detailed descriptions of tool parameters for LLM consumption.
 * Provides customizable formatting options for parameter descriptions.
 *
 * @remarks
 * The tool parameters describer is responsible for generating human-readable
 * descriptions of tool parameters. It supports multiple output formats and
 * can be customized to include or exclude specific information such as
 * parameter types, defaults, and constraints.
 */

import { renderTemplate } from "../utils/template-renderer/index.js";
import {
  TOOL_PARAMETERS_SCHEMA_TEMPLATE,
  PARAMETER_DESCRIPTION_LINE_TEMPLATE,
} from "../../resources/predefined/prompt-templates/tool-parameters-templates.js";
import type { Tool } from "@wf-agent/types";

/**
 * Options for generating tool parameter descriptions
 */
export interface ToolParameterDescriptionOptions {
  /** Whether to include the tool name in the description */
  includeToolName?: boolean;
}

/**
 * Generate a full description of tool parameters
 *
 * @param tool - The tool to describe
 * @param options - Description options
 * @returns A formatted description string
 */
export function generateToolParametersDescription(
  tool: Tool,
  options: ToolParameterDescriptionOptions = {},
): string {
  const { id, description, parameters } = tool;
  const { includeToolName = true } = options;

  const parametersSchema = JSON.stringify(parameters, null, 2);
  const parametersDescription = generateSimpleParametersDescription(tool);

  if (includeToolName) {
    return renderTemplate(TOOL_PARAMETERS_SCHEMA_TEMPLATE.content, {
      toolName: id,
      toolId: id,
      toolDescription: description || "No description",
      parametersSchema,
      parametersDescription,
    });
  }

  return `Parameter schema:\n${parametersSchema}\n\nParameter description:\n${parametersDescription}`;
}

/**
 * Generate a simple parameter description (without tool name and schema)
 *
 * @param tool - The tool to describe
 * @returns A simple parameter description
 */
export function generateSimpleParametersDescription(tool: Tool): string {
  const { parameters } = tool;

  if (!parameters || !parameters.properties) {
    return "No parameters";
  }

  const requiredParams = parameters.required || [];
  const propertyEntries = Object.entries(parameters.properties);

  if (propertyEntries.length === 0) {
    return "No parameters";
  }

  const lines: string[] = [];

  for (const [key, value] of propertyEntries) {
    const isRequired = requiredParams.includes(key);
    const paramType = getParameterType(value);
    const paramDescription = value.description || "No description";
    const requiredTag = isRequired ? "(required)" : "(optional)";

    // Handle nested object parameter
    if (value.type === "object" && value.properties) {
      lines.push(`- ${key} (${paramType}): ${paramDescription} ${requiredTag}`);

      // Add nested properties
      const nestedRequired = value.required || [];
      for (const [nestedKey, nestedValue] of Object.entries(value.properties)) {
        const nestedIsRequired = nestedRequired.includes(nestedKey);
        const nestedType = getParameterType(nestedValue);
        const nestedDescription = nestedValue.description || "No description";
        const nestedRequiredTag = nestedIsRequired ? "(required)" : "(optional)";
        lines.push(`  - ${nestedKey} (${nestedType}): ${nestedDescription} ${nestedRequiredTag}`);
      }
    } else if (value.type === "array" && value.items) {
      lines.push(`- ${key} (${paramType}): ${paramDescription} ${requiredTag}`);
      // Add array element description if items is an object type
      if (typeof value.items === "object" && value.items.properties) {
        lines.push(`  Array items:`);
        const arrayItemRequired = value.items.required || [];
        for (const [itemKey, itemValue] of Object.entries(value.items.properties)) {
          const itemIsRequired = arrayItemRequired.includes(itemKey);
          const itemType = getParameterType(itemValue);
          const itemDescription = itemValue.description || "No description";
          const itemRequiredTag = itemIsRequired ? "(required)" : "(optional)";
          lines.push(
            `  - ${itemKey} (${itemType}): ${itemDescription} ${itemRequiredTag}`,
          );
        }
      }
    } else {
      lines.push(
        renderTemplate(PARAMETER_DESCRIPTION_LINE_TEMPLATE.content, {
          paramName: key,
          paramType,
          paramDescription,
          required: requiredTag,
        }),
      );
    }
  }

  return lines.join("\n");
}

/**
 * Get the type of a parameter
 *
 * @param value - The parameter value
 * @returns The parameter type string
 */
function getParameterType(value: {
  type?: string;
  properties?: Record<string, unknown>;
  items?: Record<string, unknown>;
}): string {
  if (value.type) {
    return value.type;
  }

  if (value.properties) {
    return "object";
  }

  if (value.items) {
    return "array";
  }

  return "unknown";
}

/**
 * Get required parameters from a tool
 *
 * @param tool - The tool to get required parameters from
 * @returns An array of required parameter names
 */
export function getRequiredParameters(tool: Tool): string[] {
  if (!tool.parameters || !tool.parameters.required) {
    return [];
  }

  return tool.parameters.required;
}

/**
 * Get optional parameters from a tool
 *
 * @param tool - The tool to get optional parameters from
 * @returns An array of optional parameter names
 */
export function getOptionalParameters(tool: Tool): string[] {
  if (!tool.parameters || !tool.parameters.properties) {
    return [];
  }

  const required = tool.parameters.required || [];
  return Object.keys(tool.parameters.properties).filter(key => !required.includes(key));
}

/**
 * Check if a tool has parameters
 *
 * @param tool - The tool to check
 * @returns True if the tool has parameters
 */
export function hasParameters(tool: Tool): boolean {
  if (!tool.parameters || !tool.parameters.properties) {
    return false;
  }

  return Object.keys(tool.parameters.properties).length > 0;
}