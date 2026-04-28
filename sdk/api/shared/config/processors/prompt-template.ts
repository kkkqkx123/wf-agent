/**
 * PromptTemplate Configuration Processing Functions
 * Provide functions for validating, converting, and exporting PromptTemplate configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { validatePromptTemplateConfig } from "../validators/prompt-template-validator.js";
import { ok } from "@wf-agent/common-utils";
import type { PromptTemplate } from "@wf-agent/prompt-templates";
import { loadPromptTemplateConfig, mergePromptTemplateConfig } from "../prompt-template-loader.js";

/**
 * Verify PromptTemplate configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validatePromptTemplate(
  config: ParsedConfig<"prompt_template">,
): Result<ParsedConfig<"prompt_template">, ValidationError[]> {
  const result = validatePromptTemplateConfig(config.config);

  // Use `andThen` for type conversion.
  return result.andThen(() => ok(config)) as Result<
    ParsedConfig<"prompt_template">,
    ValidationError[]
  >;
}

/**
 * Translate the given text into English:
 *
 * **Translate PromptTemplate configuration**
 * Merge the application layer configuration with the default template.
 * - `@param config` The parsed configuration object.
 */
export function transformPromptTemplate(
  config: ParsedConfig<"prompt_template">,
  defaultTemplate: PromptTemplate,
): PromptTemplate {
  return mergePromptTemplateConfig(defaultTemplate, config.config);
}

/**
 * Load and convert the PromptTemplate configuration
 * A convenient method to complete loading, verification, and conversion in one step
 * @param content: The content of the configuration file
 * @param format: The format of the configuration
 * @param defaultTemplate: The default template
 * @returns: The merged PromptTemplate
 */
export function loadAndTransformPromptTemplate(
  content: string,
  format: ConfigFormat,
  defaultTemplate: PromptTemplate,
): PromptTemplate {
  // Loading configuration
  const appConfig = loadPromptTemplateConfig(content, format);

  // Verify the configuration.
  const validationResult = validatePromptTemplateConfig(appConfig);
  if (validationResult.isErr()) {
    const errorMessages = validationResult.error.map(err => err.message).join("\n");
    throw new Error(`Prompt template configuration validation failed:\n${errorMessages}`);
  }

  // Merge configurations
  return mergePromptTemplateConfig(defaultTemplate, appConfig);
}
