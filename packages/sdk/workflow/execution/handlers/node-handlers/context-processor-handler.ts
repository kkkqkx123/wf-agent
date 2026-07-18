/**
 * Context Processor / Data Processor Node Handler (Batch-Aware)
 * Responsible for executing CONTEXT_PROCESSOR nodes and handling:
 * 1. Message operations - LLM conversation history management
 * 2. Variable operations - Workflow runtime variable aggregation and transformation
 *
 * Design Principles:
 * - Support unified message operation utility functions
 * - Support batch management and visibility scope control for messages
 * - Support variable aggregation, transformation, and batch updates
 * - Return execution results with operation-specific details
 *
 * Core Concepts (Message Operations):
 * - Visible Messages: Messages after the current batch boundary, which will be sent to the LLM
 * - Invisible Messages: Messages before the current batch boundary, stored but not sent to the LLM
 * - Message Operations: truncate, insert, replace, clear, filter
 * - Batch Management: Control message visibility via startNewBatch() and rollbackToBatch()
 *
 * Core Concepts (Variable Operations):
 * - Aggregate: Combine multiple variables into one (array/object/merge modes)
 * - Transform: Transform a variable's value using expressions
 * - Batch Update: Update multiple variables atomically
 */

import type {
  RuntimeNode,
  ContextProcessorNodeConfig,
  ContextProcessorNodeOutput,
  NamedMessageContext,
  MessageContextRegistry,
  LLMMessage,
  MessageOperationConfig,
  MessageOperationResult,
  VariableOperationConfig,
  VariableOperationOutput,
} from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { RuntimeValidationError } from "@wf-agent/types";
import {
  executeAggregate,
  executeTransform,
  executeBatchUpdate,
} from "./variable-operation-handlers.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { now } from "@wf-agent/common-utils";

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
      config: MessageOperationConfig,
      onAfterOperation?: (result: MessageOperationResult) => Promise<void>,
    ) => Promise<MessageOperationResult>;
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
              config: MessageOperationConfig,
              onAfterOperation?: (result: MessageOperationResult) => Promise<void>,
            ) => Promise<MessageOperationResult>;
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
 * Context processor / Data processor node handler
 * @param executionEntity Workflow execution entity instance (provides access to variables)
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function contextProcessorHandler(
  executionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  context: ContextProcessorHandlerContext,
): Promise<ContextProcessorNodeOutput> {
  const config = node.config as ContextProcessorNodeConfig;

  // Validate configuration: at least one operation must be specified
  if (!config.variableOperation && !config.operationConfig) {
    throw new RuntimeValidationError(
      "Either operationConfig (message) or variableOperation must be specified",
      {
        operation: "validate",
        field: "config",
      }
    );
  }

  // Route to appropriate handler based on operation type
  if (config.variableOperation) {
    // Variable operation
    return await handleVariableOperation(executionEntity, node, config.variableOperation);
  } else if (config.operationConfig) {
    // Message operation (legacy path)
    const workflowExecution = executionEntity.getWorkflowExecutionData();
    return await handleMessageOperation(workflowExecution, node, context, config);
  } else {
    throw new RuntimeValidationError(
      "Either operationConfig (message) or variableOperation must be specified",
      {
        operation: "handle",
        field: "config",
      }
    );
  }
}

/**
 * Handle variable operations
 */
async function handleVariableOperation(
  executionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  operation: VariableOperationConfig,
): Promise<VariableOperationOutput> {
  const startTime = now();
  const variableManager = executionEntity.variableStateManager;
  const workflowExecution = executionEntity.getWorkflowExecutionData();

  // Get all variables for context
  const allVariables = variableManager.getAllVariables();

  try {
    let result: { value?: unknown; modified: Array<{ name: string; newValue: unknown }> };

    if (operation.operation === "aggregate") {
      result = executeAggregate(operation, variableManager, allVariables);
    } else if (operation.operation === "transform") {
      result = executeTransform(operation, variableManager, allVariables);
    } else if (operation.operation === "batch-update") {
      result = executeBatchUpdate(
        operation,
        variableManager,
        allVariables,
        (workflowExecution as unknown) as Record<string, unknown>
      );
    } else {
      throw new RuntimeValidationError(`Unknown variable operation: ${(operation as Record<string, unknown>)['operation']}`, {
        operation: "handle",
        field: "variableOperation.operation",
      });
    }

    const executionTime = now() - startTime;

    return {
      operation: operation.operation,
      modifiedVariables: result.modified,
      executionTime,
      stats: {
        sourceVariableCount:
          operation.operation === "aggregate"
            ? operation.sourceVariables.length
            : undefined,
        aggregatedItemCount:
          operation.operation === "aggregate" &&
          Array.isArray(result.value)
            ? (result.value as unknown[]).length
            : undefined,
      },
    };
  } catch (error) {
    logger.error("Variable operation failed", {
      nodeId: node.id,
      operation: operation.operation,
      error: error instanceof Error ? error.message : String(error),
    });

    throw error;
  }
}

/**
 * Handle message operations (legacy)
 */
async function handleMessageOperation(
  workflowExecution: WorkflowExecution,
  node: RuntimeNode,
  context: ContextProcessorHandlerContext,
  config: ContextProcessorNodeConfig,
): Promise<ContextProcessorNodeOutput> {
  if (!config.operationConfig) {
    throw new RuntimeValidationError("operationConfig is required for message operations", {
      operation: "handle",
      field: "operationConfig",
    });
  }

  // 2. Determine source and target contexts
  const sourceContextId = config.sourceContext || "current";
  const targetContextId = config.targetContext || "current";

  // Get registry for context operations (single retrieval)
  const registry = (
    workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }
  ).messageContextRegistry;

  if (!registry) {
    throw new RuntimeValidationError("MessageContextRegistry not found in execution context", {
      operation: "initializeContexts",
      field: "messageContextRegistry",
    });
  }

  // Get or create source and target contexts
  const sourceContext = getOrCreateNamedContext(registry, sourceContextId);

  // Ensure target context exists before processing
  const targetContextExisted = registry.has(targetContextId);
  getOrCreateNamedContext(registry, targetContextId);

  if (!targetContextExisted) {
    logger.debug("Auto-created target context for message operations", {
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
          logger.info(
            `Targeting parent workflow execution: ${parentContext.parentId} for context processing`,
            {
              nodeId: node.id,
              parentExecutionId: parentContext.parentId,
            },
          );
        }
      }
    }
  }

  // 4. Sync source context messages to conversation manager
  if (sourceContextId !== targetContextId) {
    targetConversationManager.clearMessages(false);
    targetConversationManager.addMessages(...sourceContext.messages);
    logger.debug("Synced messages from source context to target", {
      sourceContextId,
      targetContextId,
      messageCount: sourceContext.messages.length,
    });
  } else {
    const currentMessages = targetConversationManager.getMessages();
    logger.debug("Source and target context are the same, using existing messages", {
      contextId: sourceContextId,
      messageCount: currentMessages.length,
    });
  }

  // 5. Execute message operations
  const operationResult = await targetConversationManager.executeMessageOperation(
    config.operationConfig,
    async () => {
      // Tool visibility is now managed by ToolPermissionManager
    },
  );

  // 6. Save processed messages back to target context
  const processedMessages = targetConversationManager.getMessages();
  registry.update(targetContextId, processedMessages);

  logger.debug("Saved processed messages to target context", {
    targetContextId,
    messageCount: processedMessages.length,
    nodeId: node.id,
  });

  // 7. Return message operation result
  const messageCount = processedMessages.length;

  return {
    operation: config.operationConfig.operation,
    messageCount,
    sourceContext: sourceContextId,
    targetContext: targetContextId,
    stats: {
      originalMessageCount: operationResult.stats.originalMessageCount,
      visibleMessageCount: operationResult.stats.visibleMessageCount,
      invisibleMessageCount: operationResult.stats.invisibleMessageCount,
    },
  } as ContextProcessorNodeOutput;
}
