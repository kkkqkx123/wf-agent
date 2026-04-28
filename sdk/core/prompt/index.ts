/**
 * Prompt module exports
 *
 * Unified prompt resolution utilities for both Graph and Agent-Loop.
 */

export {
  resolveSystemPrompt,
  hasSystemPrompt,
  buildSystemPromptMessage,
  type SystemPromptConfig,
} from "./system-prompt-resolver.js";

export {
  buildInitialMessages,
  hasInitialMessages,
  mergeWithHistory,
  type InitialMessagesConfig,
} from "./initial-message-builder.js";
