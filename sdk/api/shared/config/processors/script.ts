/**
 * Script Configuration Processing Functions
 * Provide functions for validating, converting, and exporting Script configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { validateScriptConfig } from "../validators/script-validator.js";
import { ok } from "@wf-agent/common-utils";
import type { Script } from "@wf-agent/types";
import { stringifyJson } from "../json-parser.js";

/**
 * Verify Script Configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateScript(
  config: ParsedConfig<"script">,
): Result<ParsedConfig<"script">, ValidationError[]> {
  const result = validateScriptConfig(config.config);

  // Use `andThen` for type conversion.
  return result.andThen(() => ok(config)) as Result<ParsedConfig<"script">, ValidationError[]>;
}

/**
 * Translate Script configuration
 * Handle parameter substitution (if applicable)
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The converted Script
 */
export function transformScript(
  config: ParsedConfig<"script">,
  parameters?: Record<string, unknown>,
): Script {
  let script = config.config;

  // If parameters are provided, perform a simple parameter substitution.
  if (parameters && Object.keys(parameters).length > 0) {
    script = processParameters(script, parameters);
  }

  return script;
}

/**
 * Export Script Configuration
 * @param script Script object
 * @param format Configuration format
 * @returns String containing the configuration file content
 */
export function exportScript(script: Script, format: ConfigFormat): string {
  switch (format) {
    case "json":
      return stringifyJson(script, true);
    case "toml":
      throw new ConfigurationError(
        "The TOML format does not support export; please use the JSON format.",
        format,
        {
          suggestion: "Use JSON instead.",
        },
      );
    default:
      throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
  }
}

/**
 * Handle parameter replacement
 * @param script: Script configuration
 * @param parameters: Parameter object
 * @returns: Processed script configuration
 */
function processParameters(script: Script, parameters: Record<string, unknown>): Script {
  // Deep clone and replace parameters
  const processed = JSON.parse(JSON.stringify(script));
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
