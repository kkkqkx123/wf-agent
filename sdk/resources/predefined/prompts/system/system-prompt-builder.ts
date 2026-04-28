/**
 * System Prompt Builder
 *
 * Provides system prompt building with integrated tool description generation.
 *
 * This module combines:
 * - Fragment-based system prompt composition
 * - Tool description generation
 * - Predefined tool description registry integration
 *
 * Usage:
 * ```ts
 * import { buildCoderSystemPromptWithTools } from "./system-prompt-builder.js";
 * import { initializeToolDescriptions } from "../../tools/index.js";
 *
 * // Initialize tool descriptions (call once at startup)
 * initializeToolDescriptions();
 *
 * // Build system prompt with tool descriptions
 * const systemPrompt = buildCoderSystemPromptWithTools(availableTools);
 * ```
 */

import type { Tool } from "@wf-agent/types";
import type { ToolDescriptionFormat } from "../../../../core/utils/tools/tool-description-generator.js";
import {
  generateToolAvailabilitySection,
  toolDescriptionRegistry,
} from "../../../../core/utils/tools/index.js";
import {
  buildCompleteSystemPrompt,
  ASSISTANT_SYSTEM_PROMPT_FRAGMENTS,
  CODER_SYSTEM_PROMPT_FRAGMENTS,
} from "../fragments/composer.js";
import { initializeFragmentRegistry, fragmentRegistry } from "../fragments/registry.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "SystemPromptBuilder" });

/**
 * System prompt type
 */
export type SystemPromptType = "assistant" | "coder";

/**
 * System prompt build options
 */
export interface SystemPromptBuildOptions {
  /** System prompt type */
  type: SystemPromptType;
  /** Available tools to include in the prompt */
  tools?: Tool[];
  /** Tool description format */
  toolFormat?: ToolDescriptionFormat;
  /** Custom prefix text */
  prefix?: string;
  /** Custom suffix text */
  suffix?: string;
  /** Additional fragment IDs to include */
  additionalFragments?: string[];
  /** Fragment IDs to exclude */
  excludeFragments?: string[];
}

/**
 * Ensure fragment registry is initialized
 */
function ensureFragmentRegistryInitialized(): void {
  if (!fragmentRegistry.has("sdk.fragments.role.assistant")) {
    initializeFragmentRegistry();
  }
}

/**
 * Build system prompt with optional tool descriptions
 *
 * @param options Build options
 * @returns Complete system prompt string
 */
export function buildSystemPrompt(options: SystemPromptBuildOptions): string {
  ensureFragmentRegistryInitialized();

  const {
    type,
    tools,
    toolFormat = "detailed",
    prefix,
    suffix,
    additionalFragments = [],
    excludeFragments = [],
  } = options;

  // Get base fragment IDs
  const baseFragments =
    type === "coder" ? CODER_SYSTEM_PROMPT_FRAGMENTS : ASSISTANT_SYSTEM_PROMPT_FRAGMENTS;

  // Filter and combine fragments
  let fragments = baseFragments.filter(id => !excludeFragments?.includes(id));
  fragments = [...fragments, ...additionalFragments];

  // Generate tool list description if tools are provided
  let toolListDescription: string | undefined;
  if (tools && tools.length > 0) {
    // Add tool usage fragment
    fragments.push("sdk.fragments.tool-usage.xml-summary");

    // Generate tool availability section
    toolListDescription = generateToolAvailabilitySection(tools, toolFormat);
  }

  // Build the complete system prompt
  return buildCompleteSystemPrompt({
    fragmentIds: fragments,
    toolListDescription,
    separator: "\n\n",
  });
}

/**
 * Build assistant system prompt with tools
 *
 * @param tools Available tools (optional)
 * @param format Tool description format
 * @returns System prompt string
 */
export function buildAssistantSystemPromptWithTools(
  tools?: Tool[],
  format: ToolDescriptionFormat = "detailed",
): string {
  return buildSystemPrompt({
    type: "assistant",
    tools,
    toolFormat: format,
  });
}

/**
 * Build coder system prompt with tools
 *
 * @param tools Available tools (optional)
 * @param format Tool description format
 * @returns System prompt string
 */
export function buildCoderSystemPromptWithTools(
  tools?: Tool[],
  format: ToolDescriptionFormat = "detailed",
): string {
  return buildSystemPrompt({
    type: "coder",
    tools,
    toolFormat: format,
  });
}

/**
 * Build minimal system prompt (just role definition)
 *
 * @param type System prompt type
 * @returns Minimal system prompt string
 */
export function buildMinimalSystemPrompt(type: SystemPromptType = "assistant"): string {
  ensureFragmentRegistryInitialized();

  const roleFragmentId =
    type === "coder" ? "sdk.fragments.role.coder" : "sdk.fragments.role.assistant";

  return buildCompleteSystemPrompt({
    fragmentIds: [roleFragmentId],
  });
}

/**
 * Check if tool descriptions are available
 *
 * @param toolIds Tool IDs to check
 * @returns Array of tool IDs that have descriptions
 */
export function getAvailableToolDescriptions(toolIds: string[]): string[] {
  return toolIds.filter(id => toolDescriptionRegistry.has(id));
}

/**
 * Initialize all required registries
 *
 * Call this once at application startup before building system prompts.
 */
export function initializeSystemPromptRegistries(): void {
  ensureFragmentRegistryInitialized();

  // Tool descriptions should be initialized separately
  // import { initializeToolDescriptions } from "../../tools/index.js";
  // initializeToolDescriptions();

  logger.debug("System prompt registries initialized");
}
