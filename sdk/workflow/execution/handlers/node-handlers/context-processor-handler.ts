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

import type { Node, ContextProcessorNodeConfig } from "@wf-agent/types";
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
  /** Thread entity (optional, used to identify the parent thread) */
  executionEntity?: {
    getParentExecutionId: () => string | undefined;
    getConversationManager: () => unknown;
  };
  /** Thread Registry (optional, used to obtain the parent thread entity) */
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
 * Context processor node handler
 * @param thread Thread instance
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function contextProcessorHandler(
  thread: WorkflowExecution,
  node: Node,
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

  // 2. Obtain the target ConversationSession
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
            executionId: thread.id,
            parentExecutionId,
          });
        }
      }
    }
  }

  // 3. Execute message operations, which are internally handled by ConversationSession/MessageHistory for tasks such as refreshing and triggering events.
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
              executionId: thread.id,
              workflowId: thread.workflowId,
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

  // 6. Get the number of processed messages
  const messageCount = targetConversationManager.getMessages().length;

  const executionTime = now() - startTime;

  return {
    operation: config.operationConfig.operation,
    messageCount,
    executionTime,
    stats: result.stats,
  };
}
