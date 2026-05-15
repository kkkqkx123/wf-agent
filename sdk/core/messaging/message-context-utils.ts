/**
 * Message Context Initialization Utilities
 * 
 * Helper functions for initializing named message contexts in workflow executions.
 */

import type { NamedMessageContext, MessageContextRegistry } from "@wf-agent/types";
import type { WorkflowConfig } from "@wf-agent/types";
import { BUILTIN_CONTEXT_IDS } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * Initialize execution context with built-in contexts
 * 
 * Creates the 'current' and optionally 'system' contexts at the start of execution.
 * Also registers any static contexts defined in the workflow configuration.
 * 
 * @param registry The message context registry
 * @param workflowConfig Optional workflow configuration containing systemMessages and staticContexts
 */
export function initializeExecutionContext(
  registry: MessageContextRegistry,
  workflowConfig?: WorkflowConfig,
): void {
  // 1. Create 'current' context (main conversation)
  registry.register({
    id: BUILTIN_CONTEXT_IDS.CURRENT,
    messages: [],
    createdAt: now(),
    updatedAt: now(),
    metadata: {
      description: 'Main conversation context',
    },
  });

  logger.debug("Initialized 'current' context", {
    contextId: BUILTIN_CONTEXT_IDS.CURRENT,
  });

  // 2. Create 'system' context if workflow defines system messages
  const systemMessages = workflowConfig?.systemMessages;
  if (systemMessages && systemMessages.length > 0) {
    registry.register({
      id: BUILTIN_CONTEXT_IDS.SYSTEM,
      messages: systemMessages,
      createdAt: now(),
      updatedAt: now(),
      metadata: {
        description: 'System instructions',
      },
    });

    logger.debug("Initialized 'system' context", {
      contextId: BUILTIN_CONTEXT_IDS.SYSTEM,
      messageCount: systemMessages.length,
    });
  }

  // 3. Register static contexts if defined
  const staticContexts = workflowConfig?.staticContexts;
  if (staticContexts && staticContexts.length > 0) {
    for (const staticCtx of staticContexts) {
      registry.register({
        id: staticCtx.id,
        messages: staticCtx.messages,
        createdAt: now(),
        updatedAt: now(),
        metadata: {
          description: staticCtx.description,
        },
      });

      logger.debug("Registered static context", {
        contextId: staticCtx.id,
        messageCount: staticCtx.messages.length,
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
