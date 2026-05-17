/**
 * Context Processor Node Handler (Batch-Aware)
 * Responsible for executing CONTEXT_PROCESSOR nodes and handling message operations such as truncation, insertion, replacement, clearing, and filtering.
 *
 * Design Principles:
 * - Use unified message operation utility functions
 * - Support batch management and visibility scope control
 * - Return execution results
 * - Automatically refresh tool visibility declarations after message operations
 *
 * Core Concepts:
 * - Visible Messages: Messages after the current batch boundary, which will be sent to the LLM
 * - Invisible Messages: Messages before the current batch boundary, stored but not sent to the LLM
 * - Message Operations: truncate (truncation), insert (insertion), replace (replacement), clear (clearing), filter (filtering)
 * - Batch Management: Control message visibility via startNewBatch() and rollbackToBatch()
 */

import type { RuntimeNode, ContextProcessorNodeConfig, NamedMessageContext, MessageContextRegistry, LLMMessage } from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * Context processor execution results
 */
export interface ContextProcessorExecutionResult {
  /** Operation Type */
  operation: string;
  /** Number of processed messages */
  messageCount: number;
  /** Execution time (in milliseconds) */
  executionTime: number;
  /** Operation statistics information */
  stats?: {
    originalMessageCount: number;
    visibleMessageCount: number;
    invisibleMessageCount: number;
  };
}

/**
 * The context processor executes the context.
 */
export interface ContextProcessorHandlerContext {
  /** Dialogue Manager */
  conversationManager: {
    executeMessageOperation: (
      config: unknown,
      callback: () => Promise<void>,
    ) => Promise<{
      stats?: {
        originalMessageCount: number;
        visibleMessageCount: number;
        invisibleMessageCount: number;
      };
    }>;
    getMessages: () => LLMMessage[];
    clearMessages: (keepSystemMessage?: boolean) => void;
    addMessages: (...messages: LLMMessage[]) => number;
  };
  /** Workflow execution entity (optional, used to identify the parent workflow execution) */
  executionEntity?: {
    getParentContext: () => { parentType: string; parentId: string } | undefined;
    getConversationManager: () => unknown;
  };
  /** Workflow Execution Registry (optional, used to obtain the parent workflow execution entity) */
  executionRegistry?: {
    get: (executionId: string) =>
      | {
          getConversationManager: () => {
            executeMessageOperation: (
              config: unknown,
              callback: () => Promise<void>,
            ) => Promise<{
              stats?: {
                originalMessageCount: number;
                visibleMessageCount: number;
                invisibleMessageCount: number;
              };
            }>;
            getMessages: () => LLMMessage[];
          };
        }
      | undefined;
  };
}

/**
 * Get or create named message context
 * Note: Caller must ensure registry exists before calling this function
 */
function getOrCreateNamedContext(
  registry: MessageContextRegistry,
  contextId: string,
): NamedMessageContext {
  let context = registry.get(contextId);
  
  if (!context) {
    // Auto-create context if it doesn't exist
    context = {
      id: contextId,
      messages: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
      metadata: { description: `Auto-created context: ${contextId}` },
    };
    registry.register(context);
  }
  
  return context;
}

/**
 * Context processor node handler
 * @param workflowExecution Workflow execution instance
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function contextProcessorHandler(
  workflowExecution: WorkflowExecution,
  node: RuntimeNode,
  context: ContextProcessorHandlerContext,
): Promise<ContextProcessorExecutionResult> {
  const config = node.config as ContextProcessorNodeConfig;
  const startTime = now();

  // 1. Verify the configuration.
  if (!config.operationConfig) {
    throw new RuntimeValidationError("operationConfig is required", {
      operation: "handle",
      field: "operationConfig",
    });
  }

  // 2. Determine source and target contexts
  const sourceContextId = config.sourceContext || 'current';
  const targetContextId = config.targetContext || 'current';
  
  // Get registry for context operations (single retrieval)
  const registry = (workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
  
  if (!registry) {
    throw new RuntimeValidationError("MessageContextRegistry not found in execution context", {
      operation: "initializeContexts",
      field: "messageContextRegistry",
    });
  }
  
  // Get or create source and target contexts
  const sourceContext = getOrCreateNamedContext(registry, sourceContextId);
  
  // Ensure target context exists before processing
  // The registry.update() method requires the context to exist (throws error if not found)
  // This call auto-creates the context if needed
  const targetContextExisted = registry.has(targetContextId);
  getOrCreateNamedContext(registry, targetContextId);
  
  if (!targetContextExisted) {
    logger.debug('Auto-created target context for message operations', {
      targetContextId,
      nodeId: node.id,
    });
  }

  // 3. Obtain the target ConversationSession
  let targetConversationManager: ContextProcessorHandlerContext["conversationManager"] =
    context.conversationManager;

  if (config.operationOptions?.target === "parent") {
    const executionEntity = context.executionEntity;
    const executionRegistry = context.executionRegistry;

    if (executionEntity && executionRegistry) {
      const parentContext = executionEntity.getParentContext();
      if (parentContext) {
        const parentExecutionEntity = executionRegistry.get(parentContext.parentId);
        if (parentExecutionEntity) {
          targetConversationManager =
            parentExecutionEntity.getConversationManager() as ContextProcessorHandlerContext["conversationManager"];
          logger.info(`Targeting parent workflow execution: ${parentContext.parentId} for context processing`, {
            nodeId: node.id,
            executionId: workflowExecution.id,
            parentExecutionId: parentContext.parentId,
          });
        }
      }
    }
  }

  // 4. Sync source context messages to conversation manager
  if (sourceContextId !== targetContextId) {
    // Load source messages into the conversation manager
    targetConversationManager.clearMessages(false);
    targetConversationManager.addMessages(...sourceContext.messages);
    logger.debug('Synced messages from source context to target', {
      sourceContextId,
      targetContextId,
      messageCount: sourceContext.messages.length,
    });
  } else {
    // Source and target are the same, messages should already be in conversation manager
    const currentMessages = targetConversationManager.getMessages();
    logger.debug('Source and target context are the same, using existing messages', {
      contextId: sourceContextId,
      messageCount: currentMessages.length,
    });
  }

  // 5. Execute message operations, which are internally handled by ConversationSession/MessageHistory for tasks such as refreshing and triggering events.
  const result = await targetConversationManager.executeMessageOperation(
    config.operationConfig,
    async () => {
      // Operation callback: Refresh the tool visibility declaration
      // Tool visibility is now managed by ToolPermissionManager and automatically reflected in LLM calls
    },
  );

  // 6. Save processed messages back to target context
  const processedMessages = targetConversationManager.getMessages();
  
  // Update the target context with processed messages
  // At this point, targetContext is guaranteed to exist due to getOrCreateNamedContext call above
  registry.update(targetContextId, processedMessages);
  
  logger.debug('Saved processed messages to target context', {
    targetContextId,
    messageCount: processedMessages.length,
    nodeId: node.id,
  });

  // 7. Get the number of processed messages
  const messageCount = processedMessages.length;

  const executionTime = now() - startTime;

  return {
    operation: config.operationConfig.operation,
    messageCount,
    executionTime,
    stats: result.stats,
  };
}
