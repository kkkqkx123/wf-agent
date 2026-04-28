/**
 * LLM Profile Configuration Processing Functions
 * Provide functions for validating, converting, and exporting LLM Profile configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { validateLLMProfileConfig } from "../validators/llm-profile-validator.js";
import { ok } from "@wf-agent/common-utils";
import type { LLMProfile } from "@wf-agent/types";
import { stringifyJson } from "../json-parser.js";

/**
 * Verify LLM Profile configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateLLMProfile(
  config: ParsedConfig<"llm_profile">,
): Result<ParsedConfig<"llm_profile">, ValidationError[]> {
  const result = validateLLMProfileConfig(config.config);

  // Use `andThen` for type conversion
  return result.andThen(() => ok(config)) as Result<ParsedConfig<"llm_profile">, ValidationError[]>;
}

/**
 * Translate LLM Profile configuration
 * Handle parameter replacement (if applicable)
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The converted LLMProfile
 */
export function transformLLMProfile(
  config: ParsedConfig<"llm_profile">,
  parameters?: Record<string, unknown>,
): LLMProfile {
  let profile = config.config;

  // If parameters are provided, perform a simple parameter substitution.
  if (parameters && Object.keys(parameters).length > 0) {
    profile = processParameters(profile, parameters);
  }

  return profile;
}

/**
 * Export LLM Profile configuration
 * @param profile LLMProfile object
 * @param format configuration format
 * @returns string containing the configuration file content
 */
export function exportLLMProfile(profile: LLMProfile, format: ConfigFormat): string {
  switch (format) {
    case "json":
      return stringifyJson(profile, true);
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
 * @param profile LLM Profile configuration
 * @param parameters Parameter object
 * @returns Processed LLM Profile configuration
 */
function processParameters(profile: LLMProfile, parameters: Record<string, unknown>): LLMProfile {
  // Deeply clone and replace parameters
  const processed = JSON.parse(JSON.stringify(profile));
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
