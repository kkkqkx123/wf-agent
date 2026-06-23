/**
 * Dynamic Prompt Injection Interface
 *
 * Unified entry point for dynamic prompt injection.
 * Integrates both stable (system) and variable (user) context layers.
 *
 * ## Two-Layer Design
 *
 * 1. **System Context** (stable, cacheable)
 *    - Time, environment, tools
 *    - Injected into system message
 *    - Enables KV cache hits
 *
 * 2. **User Context** (variable, not cached)
 *    - TODOs, state, workspace updates
 *    - Appended to last user message
 *    - No cache invalidation
 *
 * ## Usage
 *
 * ```typescript
 * const injection = await buildDynamicPromptInjection(context, config);
 *
 * // Apply injection in LLM layer:
 * // messages[0].content = staticSystem + original_system
 * // messages[-1].content = original_content + dynamicUserContext
 * ```
 */

import type { DynamicPromptContext, DynamicPromptInjection } from "@wf-agent/types";
import type { DynamicContextConfig } from "@wf-agent/types";
import { buildSystemContextPrompt } from "./system-context/index.js";
import { buildUserContextContent } from "./user-context/index.js";

/**
 * Build dynamic prompt injection
 *
 * Generates both layers of dynamic context for LLM injection.
 * The system context is stable and cacheable, while user context
 * may change frequently without affecting system message cache.
 *
 * @param context Execution runtime context
 * @param config Dynamic context configuration
 * @returns Dynamic prompt injection (system + user context)
 */
export async function buildDynamicPromptInjection(
  context: DynamicPromptContext,
  config?: DynamicContextConfig,
): Promise<DynamicPromptInjection> {
  // 1. Build stable system context
  const staticSystem = await buildSystemContextPrompt(config);

  // 2. Build variable user context
  const dynamicUserContext = context.metadata
    ? await buildUserContextContent(context.metadata as Record<string, unknown>)
    : undefined;

  return {
    staticSystem: staticSystem || undefined,
    dynamicUserContext: dynamicUserContext || undefined,
  };
}
