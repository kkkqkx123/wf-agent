/**
 * LLM Message Assistance Tool
 * Provides assistance functions such as message extraction and filtering.
 */

import type { LLMMessage } from "@wf-agent/types";

/**
 * Extract system message
 *
 * Extract the first system message from the message array
 *
 * @example
 * const systemMsg = extractSystemMessage([
 *   { role: 'system', content: 'You are helpful' },
 *   { role: 'user', content: 'Hello' }
 * ]);
 * // Result: { role: 'system', content: 'You are helpful' }
 */
export function extractSystemMessage(messages: LLMMessage[]): LLMMessage | null {
  if (!messages || messages.length === 0) {
    return null;
  }

  return messages.find(msg => msg.role === "system") || null;
}

/**
 * Filter system messages
 *
 * Remove all system messages from the message array
 *
 * @example
 * const filtered = filterSystemMessages([
 *   { role: 'system', content: 'You are helpful' },
 *   { role: 'user', content: 'Hello' }
 * ]);
 * // Result: [{ role: 'user', content: 'Hello' }]
 */
export function filterSystemMessages(messages: LLMMessage[]): LLMMessage[] {
  if (!messages || messages.length === 0) {
    return [];
  }

  return messages.filter(msg => msg.role !== "system");
}

/**
 * Extract and separate system messages
 *
 * Returns the system message and the filtered message array
 *
 * @example
 * const { systemMessage, filteredMessages } = extractAndFilterSystemMessages([
 *   { role: 'system', content: 'You are helpful' },
 *   { role: 'user', content: 'Hello' }
 * ]);
 * // systemMessage: { role: 'system', content: 'You are helpful' }
 * // filteredMessages: [{ role: 'user', content: 'Hello' }]
 */
export function extractAndFilterSystemMessages(messages: LLMMessage[]): {
  systemMessage: LLMMessage | null;
  filteredMessages: LLMMessage[];
} {
  const systemMessage = extractSystemMessage(messages);
  const filteredMessages = filterSystemMessages(messages);

  return { systemMessage, filteredMessages };
}

/**
 * Check if the message array is empty.
 */
export function isEmptyMessages(messages?: LLMMessage[]): boolean {
  return !messages || messages.length === 0;
}

/**
 * Get the last message
 */
export function getLastMessage(messages: LLMMessage[]): LLMMessage | null {
  if (!messages || messages.length === 0) {
    return null;
  }

  return messages[messages.length - 1] ?? null;
}

/**
 * Get the last user message.
 */
export function getLastUserMessage(messages: LLMMessage[]): LLMMessage | null {
  if (!messages || messages.length === 0) {
    return null;
  }

  for (let i = messages.length - 1; i >= 0; i--) {
    const msg = messages[i];
    if (msg && msg.role === "user") {
      return msg;
    }
  }

  return null;
}

/**
 * Get the last message from the assistant.
 */
export function getLastAssistantMessage(messages: LLMMessage[]): LLMMessage | null {
  if (!messages || messages.length === 0) {
    return null;
  }

  for (let i = messages.length - 1; i >= 0; i--) {
    const msg = messages[i];
    if (msg && msg.role === "assistant") {
      return msg;
    }
  }

  return null;
}

/**
 * Count the number of messages (by role)
 */
export function countMessagesByRole(messages: LLMMessage[]): Record<string, number> {
  const counts: Record<string, number> = {
    system: 0,
    user: 0,
    assistant: 0,
    tool: 0,
  };

  if (!messages || messages.length === 0) {
    return counts;
  }

  for (const msg of messages) {
    if (msg && msg.role && counts[msg.role] !== undefined) {
      counts[msg.role] = (counts[msg.role] ?? 0) + 1;
    }
  }

  return counts;
}
