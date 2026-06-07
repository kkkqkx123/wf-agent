/**
 * LLM Profile Configuration Processing Functions
 * Provide functions for validating, converting, and exporting LLM Profile configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { LLMProfile } from "@wf-agent/types";
import { LLMProfileSchema } from "@wf-agent/types";
import { substituteParameters } from "../utils/config-utils.js";

/**
 * Verify LLM Profile configuration
 * Uses Zod schema for validation
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
    const errors = result.error.issues.map((e) => 
      new ConfigurationValidationError(e.message, {
        configType: "schema",
        field: e.path.join("."),
      })
    );
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
 * Returns typed data ready for serialization.
 * @param profile LLMProfile object
 * @returns The LLM profile data ready for export
 */
export function exportLLMProfile(profile: LLMProfile): LLMProfile {
  return profile;
}
