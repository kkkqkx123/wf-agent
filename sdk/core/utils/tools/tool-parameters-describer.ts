/**
 * ToolParametersDescriber - A tool for generating descriptions of tool parameters
 * Provides the functionality to generate descriptions for tool parameter schemas
 *
 * Features:
 * - Generates descriptions for tool parameter schemas
 * - Parses the properties and required fields of tool parameters
 * - Creates a list of parameter descriptions
 */

import type { Tool, ToolProperty } from "@wf-agent/types";
import { renderTemplate } from "@wf-agent/common-utils";
import {
  TOOL_PARAMETERS_SCHEMA_TEMPLATE,
  PARAMETER_DESCRIPTION_LINE_TEMPLATE,
} from "@wf-agent/prompt-templates";

/**
 * Generates a description line for a single parameter.
 *
 * @param paramName The name of the parameter.
 * @param paramDef The definition of the parameter.
 * @param isRequired Whether the parameter is required.
 * @returns A string containing the parameter description line.
 *
 * @example
 * ```ts
 * const paramDef = { type: 'string', description: 'User name' };
 * const line = generateParameterDescriptionLine('name', paramDef, true);
 * // Result: '- name (string): User name (required)'
 * ```
 */
function generateParameterDescriptionLine(
  paramName: string,
  paramDef: ToolProperty,
  isRequired: boolean,
): string {
  const variables = {
    paramName,
    paramType: paramDef.type,
    paramDescription: paramDef.description || "No description",
    required: isRequired ? "(required)" : "(optional)",
  };

  return renderTemplate(PARAMETER_DESCRIPTION_LINE_TEMPLATE, variables);
}

/**
 * Recursively generate parameter description text
 * Supports nested objects and array types
 *
 * @param properties Definition of parameter properties
 * @param required List of required parameters
 * @param indent Indentation level
 * @returns Parameter description text
 */
function generateParametersDescription(
  properties: Record<string, ToolProperty>,
  required: string[],
  indent: number = 0,
): string {
  const lines: string[] = [];
  const indentStr = "  ".repeat(indent);

  for (const [paramName, paramDef] of Object.entries(properties)) {
    const isRequired = required.includes(paramName);
    const line = generateParameterDescriptionLine(paramName, paramDef, isRequired);
    lines.push(`${indentStr}${line}`);

    // Handling nested objects
    if (paramDef.type === "object" && paramDef.properties) {
      const nestedDescription = generateParametersDescription(
        paramDef.properties,
        paramDef.required || [],
        indent + 1,
      );
      if (nestedDescription) {
        lines.push(nestedDescription);
      }
    }

    // Handling array types (if the array elements are objects)
    if (paramDef.type === "array" && paramDef.items) {
      const items = paramDef.items;
      if (items.type === "object" && items.properties) {
        const nestedDescription = generateParametersDescription(
          items.properties,
          items.required || [],
          indent + 1,
        );
        if (nestedDescription) {
          lines.push(`${indentStr}Array items:`);
          lines.push(nestedDescription);
        }
      }
    }
  }

  return lines.join("\n");
}

/**
 * Generates descriptions for tool parameters.
 * Uses the TOOL_parameters_SCHEMA_TEMPLATE to create the parameter descriptions.
 *
 * @param tool The tool object.
 * @returns A string containing the descriptions of the tool parameters.
 *
 * @example
 * ```ts
 * const tool = {
 *   id: 'tool1',
 *   name: 'Calculator',
 *   description: 'Performs calculations',
 *   parameters: {
 *     properties: {
 *       a: { type: 'number', description: 'First number' },
 *       b: { type: 'number', description: 'Second number' }
 *     },
 *     required: ['a', 'b']
 *   }
 * };
 * const description = generateToolParametersDescription/tool);
 * // The result will include the tool name, ID, description, parameter schema, and parameter descriptions.
 * ```
 */
export function generateToolParametersDescription(tool: Tool): string {
  const parametersSchema = JSON.stringify(tool.parameters, null, 2);
  const parametersDescription = generateParametersDescription(
    tool.parameters.properties,
    tool.parameters.required,
  );

  const variables = {
    toolName: tool.name,
    toolId: tool.id,
    toolDescription: tool.description || "No description",
    parametersSchema,
    parametersDescription,
  };

  return renderTemplate(TOOL_PARAMETERS_SCHEMA_TEMPLATE.content, variables);
}

/**
 * Generates a simplified description of the parameters.
 * Only the list of parameters is generated; the complete Schema is not included.
 *
 * @param tool The tool object.
 * @returns A string with the simplified parameter description.
 *
 * @example
 * ```ts
 * const description = generateSimpleParametersDescription/tool);
 * // Result:
 * // - a (number): First number (required)
 * // - b (number): Second number (required)
 * ```
 */
export function generateSimpleParametersDescription(tool: Tool): string {
  return generateParametersDescription(tool.parameters.properties, tool.parameters.required);
}

/**
 * List of required parameters for obtaining the tool
 *
 * @param tool The tool object
 * @returns An array of required parameter names
 */
export function getRequiredParameters(tool: Tool): string[] {
  return tool.parameters.required || [];
}

/**
 * Get the list of optional parameters for the tool
 *
 * @param tool The tool object
 * @returns An array of optional parameter names
 */
export function getOptionalParameters(tool: Tool): string[] {
  const allParams = Object.keys(tool.parameters.properties);
  const requiredParams = getRequiredParameters(tool);
  return allParams.filter(param => !requiredParams.includes(param));
}

/**
 * Check if the tool has parameters
 *
 * @param tool The tool object
 * @returns Whether there are parameters
 */
export function hasParameters(tool: Tool): boolean {
  return Object.keys(tool.parameters.properties).length > 0;
}
