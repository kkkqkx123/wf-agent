/**
 * TriggerTemplate configuration processing functions
 * Provide functions for validating, converting, and exporting TriggerTemplate configurations
 * All functions are stateless pure functions
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { validateWorkflowTrigger } from "@sdk/shared/validation/trigger-validator.js";
import { ok, err } from "@wf-agent/common-utils";
import type { TriggerTemplate } from "@wf-agent/types";
import { substituteParameters } from "../config-utils.js";
import { ConfigFormat } from "../types.js";
import { parseTomlRaw } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";

/**
 * Parse trigger-template configuration from raw content.
 *
 * @param content - Raw file content (TOML or JSON string).
 * @param format - Expected format.
 * @returns The parsed TriggerTemplate.
 */
export function parseTriggerTemplate(content: string, format: ConfigFormat): TriggerTemplate {
  const raw: unknown = format === "toml" ? parseTomlRaw(content) : parseJson(content);
  return raw as TriggerTemplate;
}

/**
 * Verify TriggerTemplate configuration
 * Uses validateWorkflowTrigger for validation
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateTriggerTemplate(
  config: ParsedConfig<"trigger_template">,
): Result<ParsedConfig<"trigger_template">, ValidationError[]> {
  const template = config.config as TriggerTemplate;

  // Convert TriggerTemplate to Trigger format for validation
  const triggerForValidation = {
    id: `template-${template.name}`,
    name: template.name,
    condition: template.condition,
    action: template.action,
    enabled: template.enabled ?? true,
    maxTriggers: template.maxTriggers,
    metadata: template.metadata,
  };

  const result = validateWorkflowTrigger(triggerForValidation, "trigger_template");

  if (result.isErr()) {
    return err(result.error);
  }

  return ok(config);
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
  // substituteParameters is idempotent for empty parameters
  return substituteParameters(config.config, parameters);
}

/**
 * Export TriggerTemplate configuration
 * Returns typed data ready for serialization.
 * @param template: TriggerTemplate object
 * @returns The trigger template data ready for export
 */
export function exportTriggerTemplate(template: TriggerTemplate): TriggerTemplate {
  return template;
}
