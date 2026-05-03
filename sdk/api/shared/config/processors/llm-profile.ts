/**
 * LLM Profile Configuration Processing Functions
 * Provide functions for validating, converting, and exporting LLM Profile configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { LLMProfile } from "@wf-agent/types";
import { LLMProfileSchema } from "@wf-agent/types";
import { stringifyJson } from "../json-parser.js";
import { substituteParameters } from "../config-utils.js";

/**
 * Verify LLM Profile configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateLLMProfile(
  config: ParsedConfig<"llm_profile">,
): Result<ParsedConfig<"llm_profile">, ValidationError[]> {
  const profile = config.config as LLMProfile;
  
  // Use Zod schema for validation
  const result = LLMProfileSchema.safeParse(profile);
  
  if (!result.success) {
    const errors = result.error.issues.map((e: any) => new SchemaValidationError(e.message));
    return err(errors);
  }
  
  return ok(config);
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

  // If parameters are provided, perform parameter substitution
  if (parameters && Object.keys(parameters).length > 0) {
    profile = substituteParameters(profile, parameters);
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
