/**
 * Fragment Combination Tool
 *
 * Provides the functionality to combine system prompt fragments
 * Reuses the basic type definitions from @wf-agent/prompt-templates
 */

// Reuse basic types from the packages layer
import type { FragmentCompositionConfig } from "@wf-agent/types";
import type { FragmentRegistry } from "../../../../shared/registry/fragment-registry.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import { renderTemplate } from "@sdk/shared/utils/template-renderer/index.js";

const logger = createContextualLogger({ component: "FragmentComposer" });

/**
 * Complete system prompt configuration
 * Includes fragment combinations and optional dynamic tool descriptions
 */
export interface CompleteSystemPromptConfig {
  /** List of basic fragment IDs */
  fragmentIds: string[];
  /** Dynamic Tool Description (to be added after the tool-usage section) */
  toolListDescription?: string;
  /** Separator */
  separator?: string;
  /** Variable values for task-instruction fragments (fragmentId → variables map) */
  fragmentVariables?: Map<string, Record<string, unknown>>;
  /** When true, throw an error if any fragment is not found (default: false) */
  strict?: boolean;
  /** Fragment registry instance for resolving fragments */
  fragmentRegistry?: FragmentRegistry;
}

/**
 * Combine the fragments into a complete system prompt word
 *
 * @param config Fragment composition configuration
 * @param fragmentVariables Variable map
 * @param strict Whether to throw on missing fragments
 * @param fragmentRegistry Fragment registry instance
 * @returns The combined system prompt word
 */
export function composeSystemPrompt(
  config: FragmentCompositionConfig,
  fragmentVariables?: Map<string, Record<string, unknown>>,
  strict: boolean = false,
  fragmentRegistry?: FragmentRegistry,
): string {
  const separator = config.separator ?? "\n\n";
  const contents: string[] = [];

  for (const fragmentId of config.fragmentIds) {
    const fragment = fragmentRegistry?.get(fragmentId);
    if (fragment) {
      // Use renderTemplate() for fragments with variables to support substitution
      if (fragment.variables && fragment.variables.length > 0) {
        const variables = fragmentVariables?.get(fragmentId);
        const rendered = variables && Object.keys(variables).length > 0
          ? renderTemplate(fragment.content, variables)
          : fragment.content;
        contents.push(rendered);
      } else {
        contents.push(fragment.content);
      }
    } else {
      if (strict) {
        throw new Error(`Fragment '${fragmentId}' not found in registry`);
      }
      logger.warn(`Fragment '${fragmentId}' not found`);
    }
  }

  return contents.join(separator);
}

/**
 * Construct a complete set of system prompts (including dynamic tool descriptions)
 *
 * @param config Complete configuration
 * @returns System prompts
 */
export function buildCompleteSystemPrompt(config: CompleteSystemPromptConfig): string {
  const basePrompt = composeSystemPrompt(
    { fragmentIds: config.fragmentIds, separator: config.separator },
    config.fragmentVariables,
    config.strict,
    config.fragmentRegistry,
  );

  // If there are any instructions for the dynamic tool, add them after the tool-usage section.
  if (config.toolListDescription) {
    return `${basePrompt}\n\n### Available Tools\n\n${config.toolListDescription}\n\n### Tool Usage Rules\n1. Only use the tools listed above\n2. Follow the exact parameter schema for each tool\n3. Wait for tool execution results before making the next call`;
  }

  return basePrompt;
}

/**
 * Quick combination of fragments
 *
 * @param fragmentIds List of fragment IDs
 * @param fragmentRegistry Fragment registry instance
 * @param strict Whether to throw on missing fragments
 * @returns Combined system prompt words
 */
export function composeFragments(
  fragmentIds: string[],
  fragmentRegistry?: FragmentRegistry,
  strict: boolean = false,
): string {
  return composeSystemPrompt({ fragmentIds }, undefined, strict, fragmentRegistry);
}

/**
 * Predefined assistant system prompt phrase combinations
 */
export const ASSISTANT_SYSTEM_PROMPT_FRAGMENTS = [
  "fragments.role.assistant",
  "fragments.capability.general",
  "fragments.capability.general-principles",
  "fragments.constraint.general-interaction",
  "fragments.constraint.general",
];

/**
 * Predefined phrases for programmer assistant system prompts
 */
export const CODER_SYSTEM_PROMPT_FRAGMENTS = [
  "fragments.role.coder",
  "fragments.capability.coding",
  "fragments.capability.coding-principles",
  "fragments.capability.coding-interaction",
  "fragments.constraint.coding",
  "fragments.constraint.code-safety",
];

/**
 * Build Assistant System Prompt Words (with optional tool descriptions)
 *
 * @param toolListDescription Dynamic tool descriptions (optional)
 * @param fragmentRegistry Fragment registry instance
 * @returns System prompt words
 */
export function buildAssistantSystemPrompt(
  toolListDescription?: string,
  fragmentRegistry?: FragmentRegistry,
): string {
  const fragments = [...ASSISTANT_SYSTEM_PROMPT_FRAGMENTS];

  if (toolListDescription) {
    fragments.push("fragments.tool-usage.xml-summary");
  }

  return buildCompleteSystemPrompt({
    fragmentIds: fragments,
    toolListDescription,
    fragmentRegistry,
  });
}

/**
 * Build a programmer assistant system prompt with optional tool descriptions
 *
 * @param toolListDescription Dynamic tool descriptions (optional)
 * @param fragmentRegistry Fragment registry instance
 * @returns System prompts
 */
export function buildCoderSystemPrompt(
  toolListDescription?: string,
  fragmentRegistry?: FragmentRegistry,
): string {
  const fragments = [...CODER_SYSTEM_PROMPT_FRAGMENTS];

  if (toolListDescription) {
    fragments.push("fragments.tool-usage.xml-summary");
  }

  return buildCompleteSystemPrompt({
    fragmentIds: fragments,
    toolListDescription,
    fragmentRegistry,
  });
}
