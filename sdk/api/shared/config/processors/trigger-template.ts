/**
 * TriggerTemplate configuration processing functions
 * Provide functions for validating, converting, and exporting TriggerTemplate configurations
 * All functions are stateless pure functions
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { validateTriggerTemplateConfig } from "../validators/trigger-template-validator.js";
import { ok } from "@wf-agent/common-utils";
import type { TriggerTemplate } from "@wf-agent/types";
import { stringifyJson } from "../json-parser.js";

/**
 * Verify TriggerTemplate configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateTriggerTemplate(
  config: ParsedConfig<"trigger_template">,
): Result<ParsedConfig<"trigger_template">, ValidationError[]> {
  const result = validateTriggerTemplateConfig(config.config);

  // Use `andThen` for type conversion.
  return result.andThen(() => ok(config)) as Result<
    ParsedConfig<"trigger_template">,
    ValidationError[]
  >;
}

/**
 * Translate TriggerTemplate configuration
 * Handle parameter replacement (if applicable)
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The converted TriggerTemplate
 */
export function transformTriggerTemplate(
  config: ParsedConfig<"trigger_template">,
  parameters?: Record<string, unknown>,
): TriggerTemplate {
  let template = config.config;

  // If parameters are provided, perform a simple parameter substitution.
  if (parameters && Object.keys(parameters).length > 0) {
    template = processParameters(template, parameters);
  }

  return template;
}

/**
 * Export TriggerTemplate configuration
 * @param template: TriggerTemplate object
 * @param format: Configuration format
 * @returns: String containing the configuration file content
 */
export function exportTriggerTemplate(template: TriggerTemplate, format: ConfigFormat): string {
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
 * @param template: Trigger template
 * @param parameters: Parameter object
 * @returns: Processed trigger template
 */
function processParameters(
  template: TriggerTemplate,
  parameters: Record<string, unknown>,
): TriggerTemplate {
  // Deep clone and replace parameters
  const processed = JSON.parse(JSON.stringify(template));
  replaceParametersInObject(processed, parameters);
  return processed;
}

/**
 * Recursive replacement of parameter placeholders in an object
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
