/**
 * Prompt Registration Module
 *
 * Centralizes all prompt-related registration including:
 * - System prompt fragments
 * - Prompt templates
 *
 * This module should be used by the unified registration orchestrator.
 * Direct registration through prompts/index.ts is deprecated.
 */

import type { PromptTemplate } from "@wf-agent/types";
import type { PromptTemplateRegistry } from "../../shared/registry/prompt-template-registry.js";
import type { FragmentRegistry } from "../../shared/registry/fragment-registry.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { ResourceRegistrationResult } from "./types.js";

const logger = createContextualLogger({ component: "PromptRegistration" });

/**
 * All predefined prompt templates
 */
const ALL_PREDEFINED_PROMPT_TEMPLATES: PromptTemplate[] = [];

/**
 * All predefined fragments
 */
import {
  ASSISTANT_ROLE_FRAGMENT,
  CODER_ROLE_FRAGMENT,
  ANALYST_ROLE_FRAGMENT,
} from "../predefined/prompts/fragments/role/assistant.js";

import {
  GENERAL_CAPABILITY_FRAGMENT,
  GENERAL_WORK_PRINCIPLES_FRAGMENT,
  CODING_CAPABILITY_FRAGMENT,
  CODING_WORK_PRINCIPLES_FRAGMENT,
  CODING_INTERACTION_FRAGMENT,
} from "../predefined/prompts/fragments/capability/index.js";

import {
  GENERAL_CONSTRAINTS_FRAGMENT,
  GENERAL_INTERACTION_FRAGMENT,
  CODING_CONSTRAINTS_FRAGMENT,
  CODE_SAFETY_FRAGMENT,
} from "../predefined/prompts/fragments/constraint/index.js";

import {
  TOOL_USAGE_XML_SUMMARY_FRAGMENT,
  TOOL_USAGE_JSON_SUMMARY_FRAGMENT,
} from "../predefined/prompts/fragments/tool-usage/summary.js";

import { CODE_REVIEW_FRAGMENT, DATA_ANALYSIS_FRAGMENT } from "../predefined/prompts/user-commands/index.js";

const ALL_PREDEFINED_FRAGMENTS = [
  ASSISTANT_ROLE_FRAGMENT,
  CODER_ROLE_FRAGMENT,
  ANALYST_ROLE_FRAGMENT,
  GENERAL_CAPABILITY_FRAGMENT,
  GENERAL_WORK_PRINCIPLES_FRAGMENT,
  CODING_CAPABILITY_FRAGMENT,
  CODING_WORK_PRINCIPLES_FRAGMENT,
  CODING_INTERACTION_FRAGMENT,
  GENERAL_CONSTRAINTS_FRAGMENT,
  GENERAL_INTERACTION_FRAGMENT,
  CODING_CONSTRAINTS_FRAGMENT,
  CODE_SAFETY_FRAGMENT,
  TOOL_USAGE_XML_SUMMARY_FRAGMENT,
  TOOL_USAGE_JSON_SUMMARY_FRAGMENT,
  CODE_REVIEW_FRAGMENT,
  DATA_ANALYSIS_FRAGMENT,
];

export interface PromptRegOptions {
  skipIfExists?: boolean;
}

export interface PromptRegResult {
  fragments: ResourceRegistrationResult;
  templates: ResourceRegistrationResult;
}

/**
 * Register all predefined fragments to the FragmentRegistry.
 */
export function registerPredefinedFragments(
  fragmentRegistry: FragmentRegistry,
  options: PromptRegOptions = {},
): ResourceRegistrationResult {
  const skipIfExists = options.skipIfExists ?? true;
  const success: string[] = [];
  const failures: Array<{ id: string; error: string }> = [];

  for (const fragment of ALL_PREDEFINED_FRAGMENTS) {
    try {
      if (skipIfExists && fragmentRegistry.has(fragment.id)) {
        logger.debug(`Fragment already registered, skipping: ${fragment.id}`);
        continue;
      }
      fragmentRegistry.register(fragment.id, fragment, { skipIfExists });
      success.push(fragment.id);
      logger.debug(`Registered fragment: ${fragment.id}`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ id: fragment.id, error: errorMsg });
      logger.error(`Failed to register fragment: ${fragment.id}`, { error: errorMsg });
    }
  }

  logger.info(`Fragments registration completed: ${success.length} succeeded, ${failures.length} failed`);
  return { success, failures };
}

/**
 * Register all predefined prompt templates.
 * Fragments must be registered before templates for cross-reference validation.
 */
export function registerPredefinedPromptTemplates(
  promptTemplateRegistry: PromptTemplateRegistry,
  options: PromptRegOptions = {},
): ResourceRegistrationResult {
  const skipIfExists = options.skipIfExists ?? true;
  const success: string[] = [];
  const failures: Array<{ id: string; error: string }> = [];

  for (const template of ALL_PREDEFINED_PROMPT_TEMPLATES) {
    try {
      if (skipIfExists && promptTemplateRegistry.has(template.id)) {
        logger.debug(`Template already registered, skipping: ${template.id}`);
        continue;
      }
      promptTemplateRegistry.register(template.id, template, { skipIfExists });
      success.push(template.id);
      logger.debug(`Registered prompt template: ${template.id}`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ id: template.id, error: errorMsg });
      logger.error(`Failed to register prompt template: ${template.id}`, { error: errorMsg });
    }
  }

  logger.info(`Prompt templates registration completed: ${success.length} succeeded, ${failures.length} failed`);
  return { success, failures };
}

/**
 * Register all predefined prompts (fragments + templates).
 * Ensures fragments are registered before templates.
 */
export function registerAllPredefinedPrompts(
  promptTemplateRegistry: PromptTemplateRegistry,
  fragmentRegistry: FragmentRegistry,
  options: PromptRegOptions = {},
): PromptRegResult {
  const fragmentResult = registerPredefinedFragments(fragmentRegistry, options);

  // Set up cross-reference: templates can reference fragments
  promptTemplateRegistry.setFragmentRegistry(fragmentRegistry);

  const templateResult = registerPredefinedPromptTemplates(promptTemplateRegistry, options);

  return {
    fragments: fragmentResult,
    templates: templateResult,
  };
}

/**
 * Check if fragments are registered.
 */
export function areFragmentsRegistered(fragmentRegistry: FragmentRegistry): boolean {
  return ALL_PREDEFINED_FRAGMENTS.every(f => fragmentRegistry.has(f.id));
}

/**
 * Check if prompt templates are registered.
 */
export function arePromptTemplatesRegistered(promptTemplateRegistry: PromptTemplateRegistry): boolean {
  return ALL_PREDEFINED_PROMPT_TEMPLATES.every(t => promptTemplateRegistry.has(t.id));
}
