/**
 * TriggerTemplate configuration processing functions
 * Provide functions for validating, converting, and exporting TriggerTemplate configurations
 * All functions are stateless pure functions
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { validateWorkflowTrigger } from "../../../../core/validation/trigger-validator.js";
import { ok, err } from "@wf-agent/common-utils";
import type { TriggerTemplate } from "@wf-agent/types";
import { stringifyJson } from "../parsers/json-parser.js";
import { substituteParameters } from "../utils/config-utils.js";

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
  let template = config.config;

  // If parameters are provided, perform parameter substitution
  if (parameters && Object.keys(parameters).length > 0) {
    template = substituteParameters(template, parameters);
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
