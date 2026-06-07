/**
 * PromptTemplate Configuration Processing Functions
 * Provide functions for validating, converting, and exporting PromptTemplate configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { PromptTemplate } from "@wf-agent/types";
import type { PromptTemplateConfigFile } from "../types.js";
import { PromptTemplateSchema } from "@wf-agent/types";
import { mergePromptTemplateConfig } from "../loaders/prompt-template-config-loader.js";

/**
 * Verify PromptTemplate configuration
 * Uses Zod schema for validation
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
 * Returns typed data ready for serialization.
 * @param template PromptTemplate object
 * @returns The prompt template data ready for export
 */
export function exportPromptTemplate(template: PromptTemplate): PromptTemplate {
  return template;
}
