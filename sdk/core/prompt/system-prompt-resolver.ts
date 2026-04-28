/**
 * System Prompt Resolver
 *
 * Unified system prompt resolution for both Graph and Agent-Loop modules.
 * Supports multiple prompt sources: templateId, direct string, or default template.
 *
 * Design Principles:
 * - Unified interface for both Graph and Agent-Loop
 * - Priority: templateId > direct string > default template
 * - Reusable across all execution contexts
 * - Type-safe configuration
 */

import { templateRegistry } from "../../resources/predefined/template-registry.js";
import { sdkLogger as logger } from "../../utils/logger.js";

/**
 * System Prompt Configuration
 *
 * Unified configuration interface for system prompt resolution.
 * Used by both Graph LLM nodes and Agent-Loop execution.
 */
export interface SystemPromptConfig {
  /** Direct system prompt string */
  systemPrompt?: string;
  /** System prompt template ID (takes priority over systemPrompt) */
  systemPromptTemplateId?: string;
  /** Template variables for rendering */
  systemPromptTemplateVariables?: Record<string, unknown>;
}

/**
 * Resolve system prompt from configuration
 *
 * Resolution priority:
 * 1. If systemPromptTemplateId is provided, use template registry
 * 2. If systemPrompt is provided, use it directly
 * 3. Otherwise, return empty string (no system prompt)
 *
 * @param config System prompt configuration
 * @returns Resolved system prompt string
 */
export function resolveSystemPrompt(config: SystemPromptConfig): string {
  // 1. Priority: templateId
  if (config.systemPromptTemplateId) {
    const rendered = templateRegistry.render(
      config.systemPromptTemplateId,
      config.systemPromptTemplateVariables || {},
    );

    if (rendered !== null) {
      return rendered;
    }

    // Template not found, log warning and fall through
    logger.warn(
      `System prompt template '${config.systemPromptTemplateId}' not found, falling back to direct systemPrompt or empty string`,
      { templateId: config.systemPromptTemplateId },
    );
  }

  // 2. Fallback: direct string
  if (config.systemPrompt) {
    return config.systemPrompt;
  }

  // 3. No system prompt configured
  return "";
}

/**
 * Check if system prompt is configured
 *
 * @param config System prompt configuration
 * @returns True if any system prompt source is configured
 */
export function hasSystemPrompt(config: SystemPromptConfig): boolean {
  return !!(config.systemPromptTemplateId || config.systemPrompt);
}

/**
 * Build system prompt message
 *
 * Creates a system message object from configuration.
 * Returns null if no system prompt is configured.
 *
 * @param config System prompt configuration
 * @returns System message object or null
 */
export function buildSystemPromptMessage(
  config: SystemPromptConfig,
): { role: "system"; content: string } | null {
  const content = resolveSystemPrompt(config);
  if (!content) {
    return null;
  }
  return { role: "system", content };
}
