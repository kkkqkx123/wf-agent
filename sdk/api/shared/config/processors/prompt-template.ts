/**
 * PromptTemplate Configuration Processing Functions
 * Provide functions for validating, converting, and exporting PromptTemplate configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { PromptTemplate } from "@wf-agent/prompt-templates";
import { PromptTemplateSchema } from "@wf-agent/prompt-templates";
import type { PromptTemplateConfigFile } from "../types.js";
import { loadPromptTemplateConfig, mergePromptTemplateConfig } from "../prompt-template-loader.js";
import { stringifyJson } from "../json-parser.js";

/**
 * Verify PromptTemplate configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validatePromptTemplate(
  config: ParsedConfig<"prompt_template">,
): Result<ParsedConfig<"prompt_template">, ValidationError[]> {
  const cfg = config.config as PromptTemplateConfigFile;
  
  // Use Zod schema for validation
  const result = PromptTemplateSchema.safeParse(cfg);
  
  if (!result.success) {
    const errors = result.error.issues.map((e: any) => new SchemaValidationError(e.message));
    return err(errors);
  }
  
  return ok(config);
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
 * Export PromptTemplate configuration
 * @param template PromptTemplate object
 * @param format Configuration format
 * @returns String containing the configuration file content
 */
export function exportPromptTemplate(template: PromptTemplate, format: ConfigFormat): string {
  switch (format) {
    case "json":
      return stringifyJson(template, true);
    case "toml":
      throw new ConfigurationError(
        "TOML format does not support export, please use JSON format",
        format,
        {
          suggestion: "Use JSON instead",
        },
      );
    default:
      throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
  }
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
  const validationResult = validatePromptTemplate({
    configType: "prompt_template",
    format,
    config: appConfig,
    rawContent: content,
  });
  if (validationResult.isErr()) {
    const errorMessages = validationResult.error.map((err: ValidationError) => err.message).join("\n");
    throw new Error(`Prompt template configuration validation failed:\n${errorMessages}`);
  }

  // Merge configurations
  return mergePromptTemplateConfig(defaultTemplate, appConfig);
}
