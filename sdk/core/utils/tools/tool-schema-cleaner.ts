/**
 * ToolSchemaCleaner - A tool for cleaning schema data
 * Provides the functionality to clean schema parameters, ensuring compatibility with various LLMs (Large Language Models).
 *
 * Features:
 * - Removes fields that are incompatible with Gemini.
 * - Removes fields that are incompatible with Anthropic.
 * - Removes fields that are incompatible with OpenAI.
 * - Recursively processes nested objects.
 */

import type { ToolParameterSchema, ToolProperty } from "@wf-agent/types";

/**
 * LLM Providers Types
 */
export type LLMProvider = "openai" | "anthropic" | "gemini";

/**
 * Common fields that need to be removed (not supported by all LLMs)
 */
const COMMON_UNSUPPORTED_FIELDS = new Set(["$schema", "$id", "$comment", "$defs", "definitions"]);

/**
 * Fields not supported by Gemini
 */
const GEMINI_UNSUPPORTED_FIELDS = new Set([
  ...COMMON_UNSUPPORTED_FIELDS,
  "additionalProperties",
  "patternProperties",
  "propertyNames",
  "if",
  "then",
  "else",
  "allOf",
  "oneOf",
  "anyOf",
  "not",
  "contentMediaType",
  "contentEncoding",
  "examples",
  "default", // Gemini has limited support for the default settings.
]);

/**
 * Fields not supported by Anthropic
 */
const ANTHROPIC_UNSUPPORTED_FIELDS = new Set([
  ...COMMON_UNSUPPORTED_FIELDS,
  "patternProperties",
  "propertyNames",
  "if",
  "then",
  "else",
  "allOf",
  "oneOf",
  "anyOf",
  "not",
]);

/**
 * Fields not supported by OpenAI
 */
const OPENAI_UNSUPPORTED_FIELDS = new Set([
  ...COMMON_UNSUPPORTED_FIELDS,
  "patternProperties",
  "propertyNames",
  "if",
  "then",
  "else",
  "allOf",
  "oneOf",
  "anyOf",
  "not",
]);

/**
 * Clean a single property definition
 *
 * @param property Definition of the property
 * @param unsupportedFields Set of unsupported fields
 * @returns The cleaned property definition
 */
function cleanProperty(property: ToolProperty, unsupportedFields: Set<string>): ToolProperty {
  const cleaned: ToolProperty = {
    type: property.type,
  };

  // Copy the supported fields.
  if (property.description !== undefined) {
    cleaned.description = property.description;
  }
  if (property.enum !== undefined) {
    cleaned.enum = property.enum;
  }
  if (property.format !== undefined) {
    cleaned.format = property.format;
  }
  if (property.default !== undefined && !unsupportedFields.has("default")) {
    cleaned.default = property.default;
  }
  if (property.examples !== undefined && !unsupportedFields.has("examples")) {
    cleaned.examples = property.examples;
  }
  if (property.minLength !== undefined) {
    cleaned.minLength = property.minLength;
  }
  if (property.maxLength !== undefined) {
    cleaned.maxLength = property.maxLength;
  }
  if (property.minimum !== undefined) {
    cleaned.minimum = property.minimum;
  }
  if (property.maximum !== undefined) {
    cleaned.maximum = property.maximum;
  }
  if (property.pattern !== undefined) {
    cleaned.pattern = property.pattern;
  }

  // Handling array types
  if (property.type === "array" && property.items) {
    cleaned.items = cleanProperty(property.items, unsupportedFields);
  }

  // Handling object types
  if (property.type === "object") {
    if (property.properties) {
      cleaned.properties = {};
      for (const [key, value] of Object.entries(property.properties)) {
        cleaned.properties[key] = cleanProperty(value, unsupportedFields);
      }
    }
    if (property.required !== undefined) {
      cleaned.required = property.required;
    }
    // Processing of additionalProperties
    if (
      property.additionalProperties !== undefined &&
      !unsupportedFields.has("additionalProperties")
    ) {
      if (typeof property.additionalProperties === "boolean") {
        cleaned.additionalProperties = property.additionalProperties;
      } else {
        cleaned.additionalProperties = cleanProperty(
          property.additionalProperties,
          unsupportedFields,
        );
      }
    }
  }

  return cleaned;
}

/**
 * Clean Tool Parameters Schema
 *
 * @param parameters: The Schema of the tool parameters
 * @param unsupportedFields: A collection of unsupported fields
 * @returns: The cleaned parameters Schema
 */
function cleanParameters(
  parameters: ToolParameterSchema,
  unsupportedFields: Set<string>,
): ToolParameterSchema {
  const cleaned: ToolParameterSchema = {
    type: "object",
    properties: {},
    required: parameters.required || [],
  };

  for (const [key, value] of Object.entries(parameters.properties)) {
    cleaned.properties[key] = cleanProperty(value, unsupportedFields);
  }

  return cleaned;
}

/**
 * Clean tool parameter schema for Gemini
 * Removes fields not supported by Gemini
 *
 * @param parameters Tool parameter schema
 * @returns Cleaned parameter schema
 *
 * @example
 * ```ts
 * const parameters = {
 *   type: 'object',
 *   properties: {
 *     name: { type: 'string', description: 'Name', $schema: '...' }
 *   },
 *   required: ['name'],
 *   additionalProperties: false
 * };
 * const cleaned = cleanForGemini(parameters);
 * // Result: additionalProperties and $schema are removed
 * ```
 */
export function cleanForGemini(parameters: ToolParameterSchema): ToolParameterSchema {
  return cleanParameters(parameters, GEMINI_UNSUPPORTED_FIELDS);
}

/**
 * Clean up the tool parameter Schema for Anthropic
 * Remove fields that are not supported by Anthropic
 *
 * @param parameters The tool parameter Schema
 * @returns The cleaned parameter Schema
 */
export function cleanForAnthropic(parameters: ToolParameterSchema): ToolParameterSchema {
  return cleanParameters(parameters, ANTHROPIC_UNSUPPORTED_FIELDS);
}

/**
 * Clean up the tool parameters Schema for OpenAI
 * Remove fields that are not supported by OpenAI
 *
 * @param parameters The tool parameters Schema
 * @returns The cleaned parameters Schema
 */
export function cleanForOpenAI(parameters: ToolParameterSchema): ToolParameterSchema {
  return cleanParameters(parameters, OPENAI_UNSUPPORTED_FIELDS);
}

/**
 * Translate from auto to en:
 *
 * Clean Tool Parameter Schema Based on the Provider
 *
 * @param parameters: The Schema of the tool parameters
 */
export function cleanForProvider(
  parameters: ToolParameterSchema,
  provider: LLMProvider,
): ToolParameterSchema {
  switch (provider) {
    case "gemini":
      return cleanForGemini(parameters);
    case "anthropic":
      return cleanForAnthropic(parameters);
    case "openai":
      return cleanForOpenAI(parameters);
    default:
      return cleanForOpenAI(parameters);
  }
}

/**
 * Clean Tool Definition
 * Returns a new tool definition with the parameter Schema having been cleaned.
 *
 * @param tool: The tool object
 * @param provider: The LLM provider
 * @returns: The cleaned tool object
 */
export function cleanToolForProvider<T extends { parameters: ToolParameterSchema }>(
  tool: T,
  provider: LLMProvider,
): T {
  return {
    ...tool,
    parameters: cleanForProvider(tool.parameters, provider),
  };
}

/**
 * Batch tool definition cleanup
 *
 * @param tools Array of tools
 * @param provider LLM provider
 * @returns Array of cleaned tools
 */
export function cleanToolsForProvider<T extends { parameters: ToolParameterSchema }>(
  tools: T[],
  provider: LLMProvider,
): T[] {
  return tools.map(tool => cleanToolForProvider(tool, provider));
}
