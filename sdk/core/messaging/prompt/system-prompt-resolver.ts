import { templateRegistry } from "../../../resources/predefined/template-registry.js";
import type { SkillRegistry } from "../../registry/skill-registry.js";
import type { SkillLoader } from "../../utils/skill-loader.js";
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

/**
 * Inject skill metadata into a resolved system prompt.
 *
 * - If prompt contains {SKILLS_METADATA}, replace with metadata
 * - If skills exist and no placeholder, append metadata at the end
 * - If no skills configured or metadata is empty, remove placeholder if present
 */
export function injectSkillMetadata(
  systemPrompt: string,
  registry: SkillRegistry,
  loader: SkillLoader,
): string {
  const metadataPrompt = loader.generateMetadataPrompt();

  if (!metadataPrompt) {
    return systemPrompt.replace("{SKILLS_METADATA}", "");
  }

  if (systemPrompt.includes("{SKILLS_METADATA}")) {
    return systemPrompt.replace("{SKILLS_METADATA}", metadataPrompt);
  }

  if (registry.getEnabledSkills().length === 0) {
    return systemPrompt;
  }

  return `${systemPrompt}\n\n${metadataPrompt}`;
}
