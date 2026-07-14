/**
 * System Prompt Builder
 *
 * Provides system prompt building with integrated tool description generation.
 *
 * This module combines:
 * - Fragment-based system prompt composition
 * - Tool description generation
 *
 * Fragments and tool descriptions must be registered via the unified registration
 * module (registration/orchestrator.ts) before using this builder.
 *
 * Usage:
 * ```ts
 * import { buildCoderSystemPromptWithTools } from "./system-prompt-builder.js";
 * import { GlobalContext } from "@sdk/shared/global-context.js";
 *
 * // Build system prompt with tool descriptions
 * const systemPrompt = buildCoderSystemPromptWithTools(globalContext, availableTools);
 * ```
 */

import type { Tool } from "@wf-agent/types";
import type { ToolDescriptionFormat } from "@sdk/shared/tools/tool-description-generator.js";
import type { GlobalContext } from "../../../../shared/global-context.js";
import {
  generateToolAvailabilitySection,
  toolDescriptionRegistry,
} from "../../../../shared/tools/index.js";
import {
  buildCompleteSystemPrompt,
  ASSISTANT_SYSTEM_PROMPT_FRAGMENTS,
  CODER_SYSTEM_PROMPT_FRAGMENTS,
} from "../fragments/composer.js";

/**
 * System prompt type
 */
export type SystemPromptType = "assistant" | "coder";

/**
 * System prompt build options
 */
export interface SystemPromptBuildOptions {
  /** Global context for accessing registries */
  globalContext?: GlobalContext;
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
  /** Variable values for task-instruction fragments (fragmentId → variables map) */
  fragmentVariables?: Map<string, Record<string, unknown>>;
}

/**
 * Build system prompt with optional tool descriptions
 *
 * @param options Build options
 * @returns Complete system prompt string
 */
export function buildSystemPrompt(options: SystemPromptBuildOptions): string {
  const { globalContext, type, tools, toolFormat = "detailed", additionalFragments = [], excludeFragments = [], fragmentVariables } = options;

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
    fragments.push("fragments.tool-usage.xml-summary");

    // Generate tool availability section
    toolListDescription = generateToolAvailabilitySection(tools, toolFormat);
  }

  // Build the complete system prompt
  return buildCompleteSystemPrompt({
    fragmentIds: fragments,
    toolListDescription,
    separator: "\n\n",
    fragmentVariables,
    fragmentRegistry: globalContext?.fragmentRegistry ?? undefined,
  });
}

/**
 * Build assistant system prompt with tools
 *
 * @param globalContext Global context for accessing registries
 * @param tools Available tools (optional)
 * @param format Tool description format
 * @returns System prompt string
 */
export function buildAssistantSystemPromptWithTools(
  globalContext?: GlobalContext,
  tools?: Tool[],
  format: ToolDescriptionFormat = "detailed",
): string {
  return buildSystemPrompt({
    globalContext,
    type: "assistant",
    tools,
    toolFormat: format,
  });
}

/**
 * Build coder system prompt with tools
 *
 * @param globalContext Global context for accessing registries
 * @param tools Available tools (optional)
 * @param format Tool description format
 * @returns System prompt string
 */
export function buildCoderSystemPromptWithTools(
  globalContext?: GlobalContext,
  tools?: Tool[],
  format: ToolDescriptionFormat = "detailed",
): string {
  return buildSystemPrompt({
    globalContext,
    type: "coder",
    tools,
    toolFormat: format,
  });
}

/**
 * Build minimal system prompt (just role definition)
 *
 * @param type System prompt type
 * @param globalContext Global context for accessing registries
 * @returns Minimal system prompt string
 */
export function buildMinimalSystemPrompt(
  type: SystemPromptType = "assistant",
  globalContext?: GlobalContext,
): string {
  const roleFragmentId = type === "coder" ? "fragments.role.coder" : "fragments.role.assistant";

  return buildCompleteSystemPrompt({
    fragmentIds: [roleFragmentId],
    fragmentRegistry: globalContext?.fragmentRegistry ?? undefined,
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
