/**
 * Message Context Initialization Utilities
 *
 * Helper functions for initializing named message contexts in workflow executions.
 */

import type { LLMMessage, NamedMessageContext, MessageContextRegistry } from "@wf-agent/types";
import type { WorkflowConfig } from "@wf-agent/types";
import { BUILTIN_CONTEXT_IDS } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { resolveSystemPrompt } from "./prompt/system-prompt-resolver.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * Initialize execution context with built-in contexts
 *
 * Creates the 'current' context at the start of execution.
 * Pre-populates it with any initial messages defined in the workflow configuration.
 * Also registers any static contexts defined in the workflow configuration.
 *
 * Note: 'system' context is no longer created separately.
 * Initial messages (including system-role messages) are placed directly into
 * the 'current' context.
 *
 * initialMessages and systemPrompt fields are mutually exclusive:
 * - If initialMessages is set, use those directly.
 * - Else if systemPromptTemplateId is set, resolve and prepend as system message.
 * - Else if systemPrompt is set, use it as system message.
 *
 * @param registry The message context registry
 * @param workflowConfig Optional workflow configuration containing initialMessages and staticContexts
 */
export function initializeExecutionContext(
  registry: MessageContextRegistry,
  workflowConfig?: WorkflowConfig,
): void {
  // 1. Resolve initial messages: initialMessages takes priority, fall back to system prompt
  let initialMessages: LLMMessage[];

  if (workflowConfig?.initialMessages && workflowConfig.initialMessages.length > 0) {
    initialMessages = [...workflowConfig.initialMessages];
  } else if (workflowConfig?.systemPromptTemplateId || workflowConfig?.systemPrompt) {
    const systemPrompt = resolveSystemPrompt({
      systemPromptTemplateId: workflowConfig.systemPromptTemplateId,
      systemPromptTemplateVariables: workflowConfig.systemPromptTemplateVariables,
      systemPrompt: workflowConfig.systemPrompt,
    });
    initialMessages = systemPrompt ? [{ role: "system", content: systemPrompt }] : [];
  } else {
    initialMessages = [];
  }

  registry.register({
    id: BUILTIN_CONTEXT_IDS.CURRENT,
    messages: [...initialMessages],
    createdAt: now(),
    updatedAt: now(),
    metadata: {
      description: "Main conversation context",
    } as Record<string, unknown>,
  });

  logger.debug("Initialized 'current' context", {
    contextId: BUILTIN_CONTEXT_IDS.CURRENT,
    initialMessageCount: initialMessages.length,
  });

  // 2. Register static contexts if defined
  const staticContexts = workflowConfig?.staticContexts;
  if (staticContexts && staticContexts.length > 0) {
    for (const staticCtx of staticContexts) {
      registry.register({
        id: staticCtx.id,
        messages: staticCtx.messages || [],
        createdAt: now(),
        updatedAt: now(),
        metadata: {
          description: staticCtx.description ?? "",
        },
      });

      logger.debug("Registered static context", {
        contextId: staticCtx.id,
        messageCount: (staticCtx.messages || []).length,
        description: staticCtx.description,
      });
    }
  }

  logger.info("Execution context initialized", {
    totalContexts: registry.listIds().length,
    contextIds: registry.listIds(),
  });
}

/**
 * Get or create a context by ID
 *
 * If the context doesn't exist, creates it with empty messages.
 * Useful for lazy initialization of target contexts.
 *
 * @param registry The message context registry
 * @param contextId The context ID
 * @returns The existing or newly created context
 */
export function getOrCreateContext(
  registry: MessageContextRegistry,
  contextId: string,
): NamedMessageContext {
  let context = registry.get(contextId);

  if (!context) {
    context = {
      id: contextId,
      messages: [],
      createdAt: now(),
      updatedAt: now(),
    };

    registry.register(context);

    logger.debug(`Auto-created context '${contextId}'`, {
      contextId,
    });
  }

  return context;
}
