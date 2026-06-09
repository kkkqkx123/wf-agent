/**
 * HookTemplate Configuration Processing Functions
 * Provide functions for validating, converting, and exporting HookTemplate configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { HookTemplate } from "@wf-agent/types";
import { substituteParameters } from "../config-utils.js";
import { ConfigFormat } from "../types.js";
import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";

/**
 * Parse hook-template configuration from raw content.
 *
 * @param content - Raw file content (TOML or JSON string).
 * @param format - Expected format.
 * @returns The parsed HookTemplate.
 */
export function parseHookTemplate(content: string, format: ConfigFormat): HookTemplate {
  const raw: unknown = format === "toml" ? parseToml(content) : parseJson(content);
  return raw as HookTemplate;
}

/**
 * Verify HookTemplate configuration.
 * Validates that the hook template has all required fields.
 *
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateHookTemplate(
  config: ParsedConfig<"hook_template">,
): Result<ParsedConfig<"hook_template">, ValidationError[]> {
  const template = config.config as HookTemplate;
  const errors: ValidationError[] = [];

  if (!template.name || typeof template.name !== "string") {
    errors.push(new ValidationError("Hook template name is required and must be a string"));
  }

  if (!template.hook) {
    errors.push(new ValidationError("Hook template 'hook' configuration is required"));
  } else {
    if (!template.hook.hookType) {
      errors.push(new ValidationError("Hook template hook.hookType is required"));
    }
    if (!template.hook.eventName) {
      errors.push(new ValidationError("Hook template hook.eventName is required"));
    }
  }

  if (errors.length > 0) {
    return err(errors) as unknown as Result<ParsedConfig<"hook_template">, ValidationError[]>;
  }

  return ok(config) as unknown as Result<ParsedConfig<"hook_template">, ValidationError[]>;
}

/**
 * Translate HookTemplate configuration.
 * Handle parameter substitution (if applicable).
 *
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The transformed HookTemplate
 */
export function transformHookTemplate(
  config: ParsedConfig<"hook_template">,
  parameters?: Record<string, unknown>,
): HookTemplate {
  // substituteParameters is idempotent for empty parameters
  return substituteParameters(config.config, parameters);
}

/**
 * Export HookTemplate configuration.
 * Returns typed data ready for serialization.
 *
 * @param template HookTemplate object
 * @returns The hook template data ready for export
 */
export function exportHookTemplate(template: HookTemplate): HookTemplate {
  return template;
}