/**
 * NodeTemplate Configuration Processing Functions
 * Provide functions for validating, converting, and exporting NodeTemplate configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { validateNodeTemplateConfig } from "../../../../workflow/validation/node-template-validation.js";
import { ok } from "@wf-agent/common-utils";
import type { NodeTemplate } from "@wf-agent/types";
import { substituteParameters } from "../utils/config-utils.js";
import { ConfigFormat } from "../types.js";
import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";

/**
 * Parse node-template configuration from raw content.
 *
 * @param content - Raw file content (TOML or JSON string).
 * @param format - Expected format.
 * @returns The parsed NodeTemplate.
 */
export function parseNodeTemplate(content: string, format: ConfigFormat): NodeTemplate {
  const raw: unknown = format === "toml" ? parseToml(content) : parseJson(content);
  return raw as NodeTemplate;
}

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
 * Returns typed data ready for serialization.
 * @param template NodeTemplate object
 * @returns The node template data ready for export
 */
export function exportNodeTemplate(template: NodeTemplate): NodeTemplate {
  return template;
}
