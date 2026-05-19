/**
 * NodeTemplate Configuration Processing Functions
 * Provide functions for validating, converting, and exporting NodeTemplate configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { validateNodeTemplateConfig } from "../../../../workflow/validation/node-template-validation.js";
import { ok } from "@wf-agent/common-utils";
import type { NodeTemplate } from "@wf-agent/types";
import { stringifyJson } from "../parsers/json-parser.js";
import { substituteParameters } from "../utils/config-utils.js";

/**
 * Verify NodeTemplate configuration
 * Delegates to the unified config validator in core/validation
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateNodeTemplate(
  config: ParsedConfig<"node_template">,
): Result<ParsedConfig<"node_template">, ValidationError[]> {
  const template = config.config as NodeTemplate;

  // Delegate to unified config validator
  const result = validateNodeTemplateConfig(template);

  // Use `andThen` for type conversion
  return result.andThen(() => ok(config)) as Result<ParsedConfig<"node_template">, ValidationError[]>;
}

/**
 * Translate NodeTemplate configuration
 * Handle parameter substitution (if applicable)
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The transformed NodeTemplate
 */
export function transformNodeTemplate(
  config: ParsedConfig<"node_template">,
  parameters?: Record<string, unknown>,
): NodeTemplate {
  let template = config.config;

  // If parameters are provided, perform parameter substitution
  if (parameters && Object.keys(parameters).length > 0) {
    template = substituteParameters(template, parameters);
  }

  return template;
}

/**
 * Export NodeTemplate configuration
 * @param template NodeTemplate object
 * @param format configuration format
 * @returns string containing the configuration file content
 */
export function exportNodeTemplate(template: NodeTemplate, format: ConfigFormat): string {
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
