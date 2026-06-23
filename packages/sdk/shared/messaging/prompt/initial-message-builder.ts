import type { LLMMessage } from "@wf-agent/types";
import { templateRegistry } from "../../../resources/predefined/template-registry.js";
import { resolveSystemPrompt, type SystemPromptConfig } from "./system-prompt-resolver.js";
import { sdkLogger as logger } from "../../../utils/logger.js";

export interface InitialMessagesConfig extends SystemPromptConfig {
  initialUserMessage?: string;
  initialUserMessageTemplateId?: string;
  initialUserMessageTemplateVariables?: Record<string, unknown>;
  existingMessages?: LLMMessage[];
  initialMessages?: LLMMessage[];
}

export function buildInitialMessages(config: InitialMessagesConfig): LLMMessage[] {
  if (config.initialMessages && config.initialMessages.length > 0) {
    return [...config.initialMessages];
  }

  const messages: LLMMessage[] = [];

  const systemPrompt = resolveSystemPrompt(config);
  if (systemPrompt) {
    messages.push({
      role: "system",
      content: systemPrompt,
    });
  }

  const userMessage = resolveInitialUserMessage(config);
  if (userMessage) {
    messages.push({
      role: "user",
      content: userMessage,
    });
  }

  if (config.existingMessages && config.existingMessages.length > 0) {
    const nonSystemMessages = config.existingMessages.filter(msg => msg.role !== "system");
    messages.push(...nonSystemMessages);
  }

  return messages;
}

function resolveInitialUserMessage(config: InitialMessagesConfig): string | null {
  if (config.initialUserMessageTemplateId) {
    const rendered = templateRegistry.render(
      config.initialUserMessageTemplateId,
      config.initialUserMessageTemplateVariables || {},
    );

    if (rendered !== null) {
      return rendered;
    }

    logger.warn(
      `Initial user message template '${config.initialUserMessageTemplateId}' not found, falling back to direct initialUserMessage`,
      { templateId: config.initialUserMessageTemplateId },
    );
  }

  if (config.initialUserMessage) {
    return config.initialUserMessage;
  }

  return null;
}
