import { templateRegistry } from "../../../resources/predefined/template-registry.js";
import { sdkLogger as logger } from "../../../utils/logger.js";
import { renderTemplate } from "../../utils/template-renderer/index.js";

export interface SystemPromptConfig {
  systemPrompt?: string;
  systemPromptTemplateId?: string;
  systemPromptTemplateVariables?: Record<string, unknown>;
}

/**
 * Resolve system prompt using template registry and variable rendering.
 *
 * Fragment composition is handled by the caller (e.g., Composer.renderSystemPrompt).
 *
 * @param config System prompt configuration
 * @returns Resolved system prompt string
 */
export function resolveSystemPrompt(config: SystemPromptConfig): string {
  let prompt: string;

  // Phase 1: Resolve from template registry
  if (config.systemPromptTemplateId) {
    const template = templateRegistry.get(config.systemPromptTemplateId);
    if (template) {
      prompt = template.content;
    } else {
      logger.warn(
        `System prompt template '${config.systemPromptTemplateId}' not found, falling back to direct systemPrompt or empty string`,
        { templateId: config.systemPromptTemplateId },
      );
      prompt = config.systemPrompt || "";
    }
  } else if (config.systemPrompt) {
    prompt = config.systemPrompt;
  } else {
    return "";
  }

  // Phase 2: Render variables
  if (config.systemPromptTemplateVariables) {
    prompt = renderTemplate(prompt, config.systemPromptTemplateVariables);
  }

  return prompt;
}
