/**
 * LLM Profile Configuration Validator
 * Provides validation logic for LLMProfile configurations
 */

import type { LLMProfile } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import {
  validateRequiredFields,
  validateStringField,
  validateEnumField,
  validateNumberField,
  validateObjectField,
} from "./validation-helpers.js";

/**
 * Validate LLM Profile configuration
 * @param profile The LLMProfile object to validate
 * @returns Validation result
 */
export function validateLLMProfileConfig(
  profile: LLMProfile,
): Result<LLMProfile, ValidationError[]> {
  const errors: ValidationError[] = [];

  // Validate required fields
  errors.push(
    ...validateRequiredFields(
      profile as unknown as Record<string, unknown>,
      ["id", "name", "provider", "model", "apiKey", "parameters"],
      "LLMProfile",
    ),
  );

  // Validate ID
  if (profile.id) {
    errors.push(
      ...validateStringField(profile.id, "LLMProfile.id", {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Validate name
  if (profile.name) {
    errors.push(
      ...validateStringField(profile.name, "LLMProfile.name", {
        minLength: 1,
        maxLength: 200,
      }),
    );
  }

  // Validate provider
  if (profile.provider) {
    errors.push(
      ...validateEnumField(profile.provider, "LLMProfile.provider", [
        "OPENAI_CHAT",
        "OPENAI_RESPONSE",
        "ANTHROPIC",
        "GEMINI_NATIVE",
        "GEMINI_OPENAI",
        "HUMAN_RELAY",
      ]),
    );
  }

  // Validate model name
  if (profile.model) {
    errors.push(
      ...validateStringField(profile.model, "LLMProfile.model", {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Validate API key
  if (profile.apiKey) {
    errors.push(
      ...validateStringField(profile.apiKey, "LLMProfile.apiKey", {
        minLength: 1,
      }),
    );
  }

  // Validate base URL (optional)
  if (profile.baseUrl !== undefined) {
    errors.push(
      ...validateStringField(profile.baseUrl, "LLMProfile.baseUrl", {
        minLength: 1,
      }),
    );
  }

  // Validate parameters object
  if (profile.parameters !== undefined) {
    errors.push(...validateObjectField(profile.parameters, "LLMProfile.parameters"));
  }

  // Validate request headers (optional)
  if (profile.headers !== undefined) {
    errors.push(...validateObjectField(profile.headers, "LLMProfile.headers"));
  }

  // Validate timeout (optional)
  if (profile.timeout !== undefined) {
    errors.push(
      ...validateNumberField(profile.timeout, "LLMProfile.timeout", {
        min: 0,
        integer: true,
      }),
    );
  }

  // Validate max retries (optional)
  if (profile.maxRetries !== undefined) {
    errors.push(
      ...validateNumberField(profile.maxRetries, "LLMProfile.maxRetries", {
        min: 0,
        integer: true,
      }),
    );
  }

  // Validate retry delay (optional)
  if (profile.retryDelay !== undefined) {
    errors.push(
      ...validateNumberField(profile.retryDelay, "LLMProfile.retryDelay", {
        min: 0,
        integer: true,
      }),
    );
  }

  // Validate metadata (optional)
  if (profile.metadata !== undefined) {
    errors.push(...validateObjectField(profile.metadata, "LLMProfile.metadata"));
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(profile);
}
