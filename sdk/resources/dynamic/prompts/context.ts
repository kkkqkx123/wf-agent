/**
 * Dynamic Context Management
 *
 * Provides functionality for storing global environment information and generating dynamic contexts.
 *
 * Design Principles:
 * - Environment information remains unchanged throughout a single execution and is stored using global variables.
 * - Information is lost upon exit, which is suitable for most practical use cases.
 * - Pure functional design, eliminating the overhead associated with class instance management.
 */

import type {
  EnvironmentInfo,
  DynamicContextConfig,
  DynamicRuntimeContext,
  DynamicContextMessage,
} from "@wf-agent/types";
import { generateEnvironmentSection, getDefaultEnvironmentInfo } from "./fragments/environment.js";
import { generateDynamicContextContent, hasDynamicContent } from "./fragments/composer.js";
import { cleanupEmptyLines } from "./fragments/utils.js";

// ============================================================================
// Global environment information storage
// ============================================================================

/**
 * Global Environment Information
 * Remains unchanged during a single execution and is lost upon exit.
 */
let globalEnvironmentInfo: EnvironmentInfo | null = null;

/**
 * Set environment information
 *
 * @param info Environment information
 */
export function setEnvironmentInfo(info: EnvironmentInfo): void {
  globalEnvironmentInfo = info;
}

/**
 * Get environment information
 *
 * If not set, return default values
 *
 * @returns Environment information
 */
export function getEnvironmentInfo(): EnvironmentInfo {
  if (globalEnvironmentInfo) {
    return globalEnvironmentInfo;
  }
  return getDefaultEnvironmentInfo();
}

/**
 * Resetting environment information
 *
 * For use in testing scenarios or when reinitialization is required
 */
export function resetEnvironmentInfo(): void {
  globalEnvironmentInfo = null;
}

// ============================================================================
// Environment Information Prompt Generation
// ============================================================================

/**
 * Generate environment information prompt words
 *
 * @returns String of environment information prompt words
 */
export function buildEnvironmentPrompt(): string {
  const envInfo = getEnvironmentInfo();
  return generateEnvironmentSection(envInfo);
}

// ============================================================================
// Dynamic context generation
// ============================================================================

/**
 * Generate dynamic context messages
 *
 * Return dynamic context messages (including time, file tree, tab pages, diagnostics, etc.)
 *
 * **Important:** These messages should only be inserted when the user actively sends a message and should not be repeatedly added in the iterative loop of AI's continuous invocation tools.
 *
 * @param config Dynamic context configuration
 * @param runtime Runtime context data
 * @returns Array of dynamic context messages
 *
 */
export function buildDynamicContextMessages(
  config: DynamicContextConfig,
  runtime?: DynamicRuntimeContext,
): DynamicContextMessage[] {
  // If no dynamic content is configured, return an empty value.
  if (!hasDynamicContent(config)) {
    return [];
  }

  const content = generateDynamicContextContent(config, runtime);
  if (!content) {
    return [];
  }

  return [
    {
      role: "user",
      content,
    },
  ];
}

/**
 * Generate dynamic context text
 *
 * For token counting
 *
 * @param config Dynamic context configuration
 * @param runtime Runtime context data
 * @returns Dynamic context text
 */
export function buildDynamicContextText(
  config: DynamicContextConfig,
  runtime?: DynamicRuntimeContext,
): string {
  const messages = buildDynamicContextMessages(config, runtime);
  return messages.map(msg => msg.content).join("\n\n");
}

// ============================================================================
// Full prompt word construction
// ============================================================================

/**
 * Construct the complete prompt phrase
 *
 * Merge the basic static prompt, environment information, and dynamic context
 *
 * @param baseStaticPrompt The basic static prompt (from the combination of fragments)
 * @param dynamicConfig The dynamic context configuration
 * @param runtime The runtime context data
 * @returns The complete prompt phrase result
 */
export function buildCompletePrompt(
  baseStaticPrompt: string,
  dynamicConfig?: DynamicContextConfig,
  runtime?: DynamicRuntimeContext,
): {
  staticPrompt: string;
  dynamicMessages: DynamicContextMessage[];
} {
  // Merge static prompts with environmental information
  const environmentPrompt = buildEnvironmentPrompt();
  const staticPrompt = environmentPrompt
    ? cleanupEmptyLines(baseStaticPrompt + "\n\n" + environmentPrompt)
    : baseStaticPrompt;

  // Get dynamic messages
  const dynamicMessages = dynamicConfig ? buildDynamicContextMessages(dynamicConfig, runtime) : [];

  return {
    staticPrompt,
    dynamicMessages,
  };
}
