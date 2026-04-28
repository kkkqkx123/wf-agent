/**
 * Tool Parameter Converter
 *
 * Converts JSON Schema format tool parameters to human-readable ToolParameterDescription format.
 *
 * This bridges the gap between:
 * - Tool.parameters (JSON Schema format for LLM function calling)
 * - ToolDescriptionData.parameters (human-readable format for prompts)
 */

import type { ToolParameterSchema, ToolProperty } from "@wf-agent/types";
import type { ToolParameterDescription } from "@wf-agent/prompt-templates";

/**
 * Convert JSON Schema property type to string representation
 */
function convertPropertyType(property: ToolProperty): string {
  const baseType = property.type;

  // Handle array with items
  if (baseType === "array" && property.items) {
    const itemType = property.items.type || "any";
    return `array<${itemType}>`;
  }

  // Handle object with properties
  if (baseType === "object" && property.properties) {
    const propNames = Object.keys(property.properties);
    if (propNames.length <= 3) {
      return `object{${propNames.join(", ")}}`;
    }
    return `object{${propNames.slice(0, 3).join(", ")}, ...}`;
  }

  // Handle enum
  if (property.enum && property.enum.length > 0) {
    const enumValues = property.enum.map(v => `"${String(v)}"`).join(" | ");
    return `${baseType}(${enumValues})`;
  }

  return baseType;
}

/**
 * Build description with constraints
 */
function buildDescription(property: ToolProperty, baseDescription?: string): string {
  const parts: string[] = [];

  if (baseDescription) {
    parts.push(baseDescription);
  }

  // Add format constraint
  if (property.format) {
    parts.push(`Format: ${property.format}`);
  }

  // Add string constraints
  if (property.minLength !== undefined) {
    parts.push(`Min length: ${property.minLength}`);
  }
  if (property.maxLength !== undefined) {
    parts.push(`Max length: ${property.maxLength}`);
  }
  if (property.pattern) {
    parts.push(`Pattern: ${property.pattern}`);
  }

  // Add number constraints
  if (property.minimum !== undefined) {
    parts.push(`Min: ${property.minimum}`);
  }
  if (property.maximum !== undefined) {
    parts.push(`Max: ${property.maximum}`);
  }

  // Add examples
  if (property.examples && property.examples.length > 0) {
    const exampleStr = property.examples.map(e => `"${String(e)}"`).join(", ");
    parts.push(`Examples: ${exampleStr}`);
  }

  return parts.join(". ");
}

/**
 * Convert a single JSON Schema property to ToolParameterDescription
 */
function convertProperty(
  name: string,
  property: ToolProperty,
  isRequired: boolean,
): ToolParameterDescription {
  return {
    name,
    type: convertPropertyType(property),
    required: isRequired,
    description: buildDescription(property, property.description),
    defaultValue: property.default,
  };
}

/**
 * Convert ToolParameterSchema (JSON Schema) to ToolParameterDescription[]
 *
 * @param parameters JSON Schema format parameters
 * @returns Human-readable parameter descriptions
 *
 * @example
 * ```ts
 * const params: ToolParameterSchema = {
 *   type: "object",
 *   properties: {
 *     path: { type: "string", description: "File path" },
 *     offset: { type: "integer", description: "Start line", default: 1 }
 *   },
 *   required: ["path"]
 * };
 *
 * const descriptions = convertToolParameters(params);
 * // [
 * //   { name: "path", type: "string", required: true, description: "File path" },
 * //   { name: "offset", type: "integer", required: false, description: "Start line", defaultValue: 1 }
 * // ]
 * ```
 */
export function convertToolParameters(parameters: ToolParameterSchema): ToolParameterDescription[] {
  if (!parameters.properties) {
    return [];
  }

  const result: ToolParameterDescription[] = [];
  const requiredSet = new Set(parameters.required || []);

  for (const [name, property] of Object.entries(parameters.properties)) {
    const isRequired = requiredSet.has(name);
    result.push(convertProperty(name, property, isRequired));
  }

  return result;
}

/**
 * Convert ToolParameterSchema to a formatted string (for simple use cases)
 *
 * @param parameters JSON Schema format parameters
 * @returns Formatted parameter description string
 */
export function convertToolParametersToString(parameters: ToolParameterSchema): string {
  const descriptions = convertToolParameters(parameters);

  if (descriptions.length === 0) {
    return "None";
  }

  return descriptions
    .map(param => {
      const req = param.required ? "required" : "optional";
      const def = param.defaultValue !== undefined ? `, default: ${param.defaultValue}` : "";
      return `  - ${param.name} (${param.type}, ${req}${def}): ${param.description}`;
    })
    .join("\n");
}

/**
 * Extract parameter names from ToolParameterSchema
 */
export function extractParameterNames(parameters: ToolParameterSchema): string[] {
  return Object.keys(parameters.properties || {});
}

/**
 * Check if a parameter is required
 */
export function isParameterRequired(parameters: ToolParameterSchema, paramName: string): boolean {
  return (parameters.required || []).includes(paramName);
}

/**
 * Get parameter default value
 */
export function getParameterDefault(
  parameters: ToolParameterSchema,
  paramName: string,
): unknown | undefined {
  const prop = parameters.properties?.[paramName];
  return prop?.default;
}
