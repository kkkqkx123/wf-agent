/**
 * Tool Parameter Converter
 *
 * Converts tool parameters into a format suitable for LLM consumption.
 * Handles various parameter schema formats and provides a unified output.
 *
 * @remarks
 * The tool parameter converter is used to convert tool parameter definitions
 * from the internal format into a format that can be easily consumed by LLMs.
 * It supports various schema formats including JSON Schema and OpenAPI-like
 * definitions.
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * Converted parameter type with boolean required flag
 */
export interface ConvertedParameter {
  type?: string;
  description?: string;
  required: boolean;
}

/**
 * Convert tool parameters schema to a readable format
 *
 * @param parameters - The tool parameters to convert
 * @returns A converted parameter object
 */
export function convertToolParameters(
  parameters: ToolParameterSchema,
): Record<string, ConvertedParameter> {
  const { properties } = parameters;

  if (!properties) {
    return {};
  }

  return Object.entries(properties).reduce(
    (acc, [key, value]) => {
      acc[key] = {
        type: value.type,
        description: value.description,
        required: parameters.required?.includes(key) ?? false,
      };
      return acc;
    },
    {} as Record<string, ConvertedParameter>,
  );
}

/**
 * Generate a summary of tool parameters
 *
 * @param parameters - The tool parameters to summarize
 * @returns A summary string
 */
export function summarizeToolParameters(parameters?: ToolParameterSchema): string {
  if (!parameters || !parameters.properties) {
    return "No parameters";
  }

  const properties = Object.entries(parameters.properties);

  if (properties.length === 0) {
    return "No parameters";
  }

  const required = parameters.required || [];

  return properties
    .map(([key, value]) => {
      const isRequired = required.includes(key);
      return `${key} (${value.type})${isRequired ? " [required]" : " [optional]"}`;
    })
    .join(", ");
}