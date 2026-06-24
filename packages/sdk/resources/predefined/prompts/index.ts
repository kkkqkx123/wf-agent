/**
 * Predefined prompt word templates are exported uniformly.
 *
 * This includes templates for system prompts, user instructions, and other business-related prompts. Additionally, segments of system prompts are exported, supporting dynamic combination.
 *
 * Registration of prompts and fragments is now handled by the unified registration module:
 * - registration/prompts-registration.ts
 *
 * This module only exports template definitions and builders.
 */

import {
  buildSystemPrompt,
  buildAssistantSystemPromptWithTools,
  buildCoderSystemPromptWithTools,
} from "./system/index.js";

// System prompt word template (based on fragment combination)
export * from "./system/index.js";

// User Instruction Template
export * from "./user-commands/index.js";

// System prompt fragments (used for dynamic combination)
export * from "./fragments/index.js";

export { buildSystemPrompt, buildAssistantSystemPromptWithTools, buildCoderSystemPromptWithTools };
