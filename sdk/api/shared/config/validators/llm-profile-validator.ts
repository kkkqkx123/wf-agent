/**
 * LLM Profile Configuration Validation Function
 * Responsible for verifying the validity of LLM Profile configurations.
 */

import type { LLMProfile } from "@wf-agent/types";
import type { ConfigFile } from "../types.js";
import { ok, err } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import {
  validateRequiredFields,
  validateStringField,
  validateEnumField,
  validateNumberField,
  validateObjectField,
} from "./base-validator.js";

/**
 * Verify LLM Profile configuration
 * @param config Configuration object
 * @returns Verification result
 */
export function validateLLMProfileConfig(
  config: ConfigFile,
): Result<LLMProfile, ValidationError[]> {
  const profile = config as LLMProfile;
  const errors: ValidationError[] = [];

  // Verify required fields
  errors.push(
    ...validateRequiredFields(
      profile as unknown as Record<string, unknown>,
      ["id", "name", "provider", "model", "apiKey", "parameters"],
      "LLMProfile",
    ),
  );

  // Verify ID
  if (profile.id) {
    errors.push(
      ...validateStringField(profile.id, "LLMProfile.id", {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Verify the name
  if (profile.name) {
    errors.push(
      ...validateStringField(profile.name, "LLMProfile.name", {
        minLength: 1,
        maxLength: 200,
      }),
    );
  }

  // Verify provider
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

  // Verify model name
  if (profile.model) {
    errors.push(
      ...validateStringField(profile.model, "LLMProfile.model", {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Verify the API key
  if (profile.apiKey) {
    errors.push(
      ...validateStringField(profile.apiKey, "LLMProfile.apiKey", {
        minLength: 1,
      }),
    );
  }

  // Verify the base URL (optional).
  if (profile.baseUrl !== undefined) {
    errors.push(
      ...validateStringField(profile.baseUrl, "LLMProfile.baseUrl", {
        minLength: 1,
      }),
    );
  }

  // Verify the parameter object
  if (profile.parameters !== undefined) {
    errors.push(...validateObjectField(profile.parameters, "LLMProfile.parameters"));
  }

  // Verify request headers (optional)
  if (profile.headers !== undefined) {
    errors.push(...validateObjectField(profile.headers, "LLMProfile.headers"));
  }

  // Verification timeout period (optional)
  if (profile.timeout !== undefined) {
    errors.push(
      ...validateNumberField(profile.timeout, "LLMProfile.timeout", {
        min: 0,
        integer: true,
      }),
    );
  }

  // Verify the maximum number of retries (optional)
  if (profile.maxRetries !== undefined) {
    errors.push(
      ...validateNumberField(profile.maxRetries, "LLMProfile.maxRetries", {
        min: 0,
        integer: true,
      }),
    );
  }

  // Verify retry delay (optional)
  if (profile.retryDelay !== undefined) {
    errors.push(
      ...validateNumberField(profile.retryDelay, "LLMProfile.retryDelay", {
        min: 0,
        integer: true,
      }),
    );
  }

  // Verify metadata (optional)
  if (profile.metadata !== undefined) {
    errors.push(...validateObjectField(profile.metadata, "LLMProfile.metadata"));
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(profile);
}
