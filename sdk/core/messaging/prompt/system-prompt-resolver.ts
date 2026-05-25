import { templateRegistry } from "../../../resources/predefined/template-registry.js";
import { sdkLogger as logger } from "../../../utils/logger.js";

export interface SystemPromptConfig {
  systemPrompt?: string;
  systemPromptTemplateId?: string;
  systemPromptTemplateVariables?: Record<string, unknown>;
}

export function resolveSystemPrompt(config: SystemPromptConfig): string {
  if (config.systemPromptTemplateId) {
    const rendered = templateRegistry.render(
      config.systemPromptTemplateId,
      config.systemPromptTemplateVariables || {},
    );

    if (rendered !== null) {
      return rendered;
    }

    logger.warn(
      `System prompt template '${config.systemPromptTemplateId}' not found, falling back to direct systemPrompt or empty string`,
      { templateId: config.systemPromptTemplateId },
    );
  }

  if (config.systemPrompt) {
    return config.systemPrompt;
  }

  return "";
}
