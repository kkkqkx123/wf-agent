/**
 * 上下文处理器节点处理器（批次感知）
 * 负责执行CONTEXT_PROCESSOR节点，处理对话消息的截断、插入、替换、清空、过滤操作
 *
 * 设计原则：
 * - 使用统一的消息操作工具函数
 * - 支持批次管理和可见范围控制
 * - 返回执行结果
 * - 消息操作后自动刷新工具可见性声明
 *
 * 核心概念：
 * - 可见消息：当前批次边界之后的消息，会被发送给LLM
 * - 不可见消息：当前批次边界之前的消息，仅存储但不发送给LLM
 * - 消息操作：truncate（截断）、insert（插入）、replace（替换）、clear（清空）、filter（过滤）
 * - 批次管理：通过 startNewBatch() 和 rollbackToBatch() 控制消息可见性
 */

import type { Node, ContextProcessorNodeConfig } from "@wf-agent/types";
import type { Thread } from "@wf-agent/types";
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
    getParentThreadId: () => string | undefined;
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
    refreshDeclaration: (threadContext: unknown) => Promise<void>;
  };
  /** Thread context (optional, used for refreshing tool visibility declarations) */
  threadContext?: unknown;
}

/**
 * Context processor node handler
 * @param thread Thread instance
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function contextProcessorHandler(
  thread: Thread,
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

    if (executionEntity && executionRegistry && executionEntity.getParentThreadId()) {
      const parentThreadId = executionEntity.getParentThreadId();
      if (parentThreadId) {
        const parentThreadEntity = executionRegistry.get(parentThreadId);
        if (parentThreadEntity) {
          targetConversationManager =
            parentThreadEntity.getConversationManager() as ContextProcessorHandlerContext["conversationManager"];
          logger.info(`Targeting parent thread: ${parentThreadId} for context processing`, {
            nodeId: node.id,
            executionId: thread.id,
            parentThreadId,
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
      if (context.toolVisibilityCoordinator && context.threadContext) {
        try {
          await context.toolVisibilityCoordinator.refreshDeclaration(context.threadContext);
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
