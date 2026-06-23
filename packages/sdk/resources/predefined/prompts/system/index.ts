/**
 * System Prompt Word Template Unified Export: In parallel with the fragments system, this module provides complete system prompt words directly, whereas fragments are in a dispersed format.
 */

// system prompt builder
export {
  buildSystemPrompt,
  buildAssistantSystemPromptWithTools,
  buildCoderSystemPromptWithTools,
  buildMinimalSystemPrompt,
  initializeSystemPromptRegistries,
  getAvailableToolDescriptions,
  type SystemPromptType,
  type SystemPromptBuildOptions,
} from "./system-prompt-builder.js";
