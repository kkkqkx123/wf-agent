/**
 * Predefined prompt word templates are exported uniformly.
 *
 * This includes templates for system prompts, user instructions, and other business-related prompts. Additionally, segments of system prompts are exported, supporting dynamic combination.
 *
 */

import type { PromptTemplate } from "@wf-agent/types";
import { templateRegistry } from "../template-registry.js";
import {
  buildSystemPrompt,
  buildAssistantSystemPromptWithTools,
  buildCoderSystemPromptWithTools,
} from "./system/index.js";
import { initializeFragmentRegistry } from "./fragments/registry.js";

// System prompt word template (based on fragment combination)
export * from "./system/index.js";

// User Instruction Template
export * from "./user-commands/index.js";

// System prompt fragments (used for dynamic combination)
export * from "./fragments/index.js";

/**
 * List of all predefined prompt word templates
 *
 * Note: User-command templates have been migrated to SystemPromptFragment
 * (task-instruction category) and are registered via FragmentRegistry.
 * See fragments/registry.ts for details.
 */
export const ALL_PREDEFINED_PROMPT_TEMPLATES: PromptTemplate[] = [];

/**
 * Register all predefined prompt word templates to the registry.
 *
 * Note: This function will automatically initialize the snippet registry.
 */
export function registerPredefinedPromptTemplates(): void {
  // First, initialize the fragment registry.
  initializeFragmentRegistry();

  // Register all templates
  for (const template of ALL_PREDEFINED_PROMPT_TEMPLATES) {
    templateRegistry.register(template);
  }
}

export { buildSystemPrompt, buildAssistantSystemPromptWithTools, buildCoderSystemPromptWithTools };
