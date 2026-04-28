/**
 * Initial Messages Builder
 *
 * Unified initial message construction for both Graph and Agent-Loop modules.
 * Builds the initial message array from configuration, including system prompt
 * and initial user messages.
 *
 * Design Principles:
 * - Unified interface for both Graph and Agent-Loop
 * - Supports system prompt via templateId or direct string
 * - Supports initial user message
 * - Supports pre-existing message history
 * - Returns immutable message array
 */

import type { LLMMessage } from "@wf-agent/types";
import { templateRegistry } from "../../resources/predefined/template-registry.js";
import { resolveSystemPrompt, type SystemPromptConfig } from "./system-prompt-resolver.js";
import { sdkLogger as logger } from "../../utils/logger.js";

/**
 * Initial Messages Configuration
 *
 * Extended configuration for building initial messages.
 * Combines system prompt config with initial user message and history.
 */
export interface InitialMessagesConfig extends SystemPromptConfig {
  /** Initial user message content */
  initialUserMessage?: string;
  /** Initial user message template ID (takes priority over initialUserMessage) */
  initialUserMessageTemplateId?: string;
  /** Template variables for initial user message */
  initialUserMessageTemplateVariables?: Record<string, unknown>;
  /** Pre-existing message history to append */
  existingMessages?: LLMMessage[];
  /** Complete initial messages array (takes highest priority) */
  initialMessages?: LLMMessage[];
}

/**
 * Build initial messages from configuration
 *
 * Message construction order:
 * 1. System prompt (if configured) - always first
 * 2. Initial user message (if configured)
 * 3. Existing messages (if provided)
 * 4. Or use initialMessages directly if provided
 *
 * @param config Initial messages configuration
 * @returns Built message array
 */
export async function buildInitialMessages(config: InitialMessagesConfig): Promise<LLMMessage[]> {
  // Highest priority: use initialMessages directly
  if (config.initialMessages && config.initialMessages.length > 0) {
    return [...config.initialMessages];
  }

  const messages: LLMMessage[] = [];

  // 1. Add system prompt (always first if present)
  const systemPrompt = resolveSystemPrompt(config);
  if (systemPrompt) {
    messages.push({
      role: "system",
      content: systemPrompt,
    });
  }

  // 2. Add initial user message
  const userMessage = await resolveInitialUserMessage(config);
  if (userMessage) {
    messages.push({
      role: "user",
      content: userMessage,
    });
  }

  // 3. Append existing messages (from history)
  if (config.existingMessages && config.existingMessages.length > 0) {
    // Filter out system messages from existing history to avoid duplication
    // (system prompt should only come from config)
    const nonSystemMessages = config.existingMessages.filter(msg => msg.role !== "system");
    messages.push(...nonSystemMessages);
  }

  return messages;
}

/**
 * Resolve initial user message from configuration
 *
 * Resolution priority:
 * 1. If initialUserMessageTemplateId is provided, use template registry
 * 2. If initialUserMessage is provided, use it directly
 * 3. Otherwise, return null
 *
 * @param config Initial messages configuration
 * @returns Resolved user message string or null
 */
async function resolveInitialUserMessage(config: InitialMessagesConfig): Promise<string | null> {
  // 1. Priority: templateId
  if (config.initialUserMessageTemplateId) {
    const rendered = templateRegistry.render(
      config.initialUserMessageTemplateId,
      config.initialUserMessageTemplateVariables || {},
    );

    if (rendered !== null) {
      return rendered;
    }

    // Template not found, log warning and fall through
    logger.warn(
      `Initial user message template '${config.initialUserMessageTemplateId}' not found, falling back to direct initialUserMessage`,
      { templateId: config.initialUserMessageTemplateId },
    );
  }

  // 2. Fallback: direct string
  if (config.initialUserMessage) {
    return config.initialUserMessage;
  }

  // 3. No initial user message configured
  return null;
}

/**
 * Check if any initial messages are configured
 *
 * @param config Initial messages configuration
 * @returns True if any initial message source is configured
 */
export function hasInitialMessages(config: InitialMessagesConfig): boolean {
  return !!(
    config.initialMessages?.length ||
    config.systemPromptTemplateId ||
    config.systemPrompt ||
    config.initialUserMessageTemplateId ||
    config.initialUserMessage ||
    config.existingMessages?.length
  );
}

/**
 * Merge initial messages with existing history
 *
 * This function is useful when you want to add system prompt to existing
 * message history without duplicating system messages.
 *
 * @param existingHistory Existing message history
 * @param config Initial messages configuration
 * @returns Merged message array
 */
export function mergeWithHistory(
  existingHistory: LLMMessage[],
  config: SystemPromptConfig,
): LLMMessage[] {
  const messages: LLMMessage[] = [];

  // Add system prompt first
  const systemPrompt = resolveSystemPrompt(config);
  if (systemPrompt) {
    messages.push({
      role: "system",
      content: systemPrompt,
    });
  }

  // Add existing history (excluding system messages to avoid duplication)
  const nonSystemMessages = existingHistory.filter(msg => msg.role !== "system");
  messages.push(...nonSystemMessages);

  return messages;
}
