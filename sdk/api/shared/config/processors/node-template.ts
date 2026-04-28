/**
 * NodeTemplate Configuration Processing Functions
 * Provide functions for validating, converting, and exporting NodeTemplate configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { validateNodeTemplateConfig } from "../validators/node-template-validator.js";
import { ok } from "@wf-agent/common-utils";
import type { NodeTemplate } from "@wf-agent/types";
import { stringifyJson } from "../json-parser.js";

/**
 * Verify NodeTemplate configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateNodeTemplate(
  config: ParsedConfig<"node_template">,
): Result<ParsedConfig<"node_template">, ValidationError[]> {
  const result = validateNodeTemplateConfig(config.config);

  // Use `andThen` for type conversion.
  return result.andThen(() => ok(config)) as Result<
    ParsedConfig<"node_template">,
    ValidationError[]
  >;
}

/**
 * Translate NodeTemplate configuration
 * Handle parameter substitution (if applicable)
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The transformed NodeTemplate
 */
export function transformNodeTemplate(
  config: ParsedConfig<"node_template">,
  parameters?: Record<string, unknown>,
): NodeTemplate {
  let template = config.config;

  // If parameters are provided, perform a simple parameter substitution.
  if (parameters && Object.keys(parameters).length > 0) {
    template = processParameters(template, parameters);
  }

  return template;
}

/**
 * Export NodeTemplate configuration
 * @param template NodeTemplate object
 * @param format configuration format
 * @returns string containing the configuration file content
 */
export function exportNodeTemplate(template: NodeTemplate, format: ConfigFormat): string {
  switch (format) {
    case "json":
      return stringifyJson(template, true);
    case "toml":
      throw new ConfigurationError(
        "TOML format does not support export, please use JSON format",
        format,
        {
          suggestion: "Use json instead of",
        },
      );
    default:
      throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
  }
}

/**
 * Handle parameter replacement
 * @param template: Node template
 * @param parameters: Parameter object
 * @returns: Processed node template
 */
function processParameters(
  template: NodeTemplate,
  parameters: Record<string, unknown>,
): NodeTemplate {
  // Deep clone and replace parameters
  const processed = JSON.parse(JSON.stringify(template));
  replaceParametersInObject(processed, parameters);
  return processed;
}

/**
 * Recursively replace parameter placeholders in an object
 * @param obj The object to be processed
 * @param parameters The parameter object
 */
function replaceParametersInObject(obj: unknown, parameters: Record<string, unknown>): void {
  if (Array.isArray(obj)) {
    for (let i = 0; i < obj.length; i++) {
      if (typeof obj[i] === "string") {
        obj[i] = replaceParameterInString(obj[i] as string, parameters);
      } else if (typeof obj[i] === "object" && obj[i] !== null) {
        replaceParametersInObject(obj[i], parameters);
      }
    }
  } else if (obj && typeof obj === "object") {
    const objRecord = obj as Record<string, unknown>;
    for (const key in objRecord) {
      if (Object.prototype.hasOwnProperty.call(objRecord, key)) {
        if (typeof objRecord[key] === "string") {
          objRecord[key] = replaceParameterInString(objRecord[key] as string, parameters);
        } else if (typeof objRecord[key] === "object" && objRecord[key] !== null) {
          replaceParametersInObject(objRecord[key], parameters);
        }
      }
    }
  }
}

/**
 * Replace parameter placeholders in the string
 * @param str The string to be processed
 * @param parameters The parameter object
 * @returns The replaced string
 */
function replaceParameterInString(str: string, parameters: Record<string, unknown>): string {
  const regex = /\{\{parameters\.(\w+)\}\}/g;
  return str.replace(regex, (match, paramName: string) => {
    if (parameters[paramName] !== undefined) {
      return String(parameters[paramName]);
    }
    return match;
  });
}
