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
import { now, getErrorOrNew } from "@wf-agent/common-utils";
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
    getMessages: () => unknown[];
  };
  /** Workflow execution entity (optional, used to identify the parent workflow execution) */
  executionEntity?: {
    getParentExecutionId: () => string | undefined;
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
            getMessages: () => unknown[];
          };
        }
      | undefined;
  };
  /** Tool Visibility Coordinator (optional) */
  toolVisibilityCoordinator?: {
    refreshDeclaration: (workflowExecutionContext: unknown) => Promise<void>;
  };
  /** Workflow execution context (optional, used for refreshing tool visibility declarations) */
  workflowExecutionContext?: unknown;
}

/**
 * Get or create named message context
 */
function getOrCreateNamedContext(
  workflowExecution: WorkflowExecution,
  contextId: string,
): NamedMessageContext {
  const registry = (workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
  
  if (!registry) {
    throw new RuntimeValidationError("MessageContextRegistry not found in execution context", {
      operation: "getOrCreateNamedContext",
      field: "messageContextRegistry",
    });
  }

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
  
  // Get source context
  const sourceContext = getOrCreateNamedContext(workflowExecution, sourceContextId);
  
  // Get target context (auto-create if needed)
  const targetContext = getOrCreateNamedContext(workflowExecution, targetContextId);

  // 3. Obtain the target ConversationSession
  let targetConversationManager: ContextProcessorHandlerContext["conversationManager"] =
    context.conversationManager;

  if (config.operationOptions?.target === "parent") {
    const executionEntity = context.executionEntity;
    const executionRegistry = context.executionRegistry;

    if (executionEntity && executionRegistry && executionEntity.getParentExecutionId()) {
      const parentExecutionId = executionEntity.getParentExecutionId();
      if (parentExecutionId) {
        const parentExecutionEntity = executionRegistry.get(parentExecutionId);
        if (parentExecutionEntity) {
          targetConversationManager =
            parentExecutionEntity.getConversationManager() as ContextProcessorHandlerContext["conversationManager"];
          logger.info(`Targeting parent workflow execution: ${parentExecutionId} for context processing`, {
            nodeId: node.id,
            executionId: workflowExecution.id,
            parentExecutionId,
          });
        }
      }
    }
  }

  // 4. Load source messages into conversation manager for processing
  // Clear current messages and load source context messages
  const currentMessages = targetConversationManager.getMessages();
  // Note: In a real implementation, we would need to sync sourceContext.messages with conversationManager
  // For now, we assume the operation will work on the current conversation state

  // 5. Execute message operations, which are internally handled by ConversationSession/MessageHistory for tasks such as refreshing and triggering events.
  const result = await targetConversationManager.executeMessageOperation(
    config.operationConfig,
    async () => {
      // Operation callback: Refresh the tool visibility declaration
      if (context.toolVisibilityCoordinator && context.workflowExecutionContext) {
        try {
          await context.toolVisibilityCoordinator.refreshDeclaration(context.workflowExecutionContext);
        } catch (error) {
          // Record warning logs without interrupting the execution.
          logger.warn(
            "Failed to refresh tool visibility declaration after message operation",
            {
              operation: config.operationConfig.operation,
              nodeId: node.id,
              executionId: workflowExecution.id,
              workflowId: workflowExecution.workflowId,
              suggestion:
                "Tool visibility may be stale. Check tool visibility coordinator configuration and retry",
            },
            undefined,
            getErrorOrNew(error),
          );
        }
      }
    },
  );

  // 6. Save processed messages back to target context
  const processedMessages = targetConversationManager.getMessages();
  const registry = (workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
  if (registry) {
    registry.update(targetContextId, processedMessages as LLMMessage[]);
  }

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
