/**
 * Application message operation processing function
 *
 * Responsible for executing application message operations (such as context compression, message truncation, etc.)
 * Obtains the ConversationSession of the thread through ThreadStateCoordinator to perform the operations
 */

import type {
  TriggerAction,
  TriggerExecutionResult,
  MessageOperationConfig,
} from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { ThreadRegistry } from "../../../stores/thread-registry.js";
import type { ThreadStateCoordinator } from "../../../state-managers/thread-state-coordinator.js";
import { getErrorMessage, now } from "@wf-agent/common-utils";

function createSuccessResult(
  triggerId: string,
  action: TriggerAction,
  data: unknown,
  executionTime: number,
): TriggerExecutionResult {
  return {
    triggerId,
    success: true,
    action,
    executionTime,
    result: data,
  };
}

function createFailureResult(
  triggerId: string,
  action: TriggerAction,
  error: unknown,
  executionTime: number,
): TriggerExecutionResult {
  return {
    triggerId,
    success: false,
    action,
    executionTime,
    error: getErrorMessage(error),
  };
}

/**
 * Apply message operation handler
 *
 * @param action Trigger action
 * @param triggerId Trigger ID
 * @param threadRegistry Thread registry
 * @param stateCoordinatorMap Optional map of ThreadStateCoordinator by threadId
 */
export async function applyMessageOperationHandler(
  action: TriggerAction,
  triggerId: string,
  threadRegistry: WorkflowExecutionRegistry,
  stateCoordinatorMap?: Map<string, ThreadStateCoordinator>,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    const { threadId, operationConfig } = action.parameters as {
      threadId?: string;
      operationConfig?: MessageOperationConfig;
    };

    if (!threadId) {
      throw new RuntimeValidationError("threadId is required for APPLY_MESSAGE_OPERATION action", {
        operation: "handle",
        field: "parameters.threadId",
      });
    }

    if (!operationConfig) {
      throw new RuntimeValidationError(
        "operationConfig is required for APPLY_MESSAGE_OPERATION action",
        { operation: "handle", field: "parameters.operationConfig" },
      );
    }

    const threadEntity = threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new Error(`Thread not found: ${threadId}`);
    }

    // Get ConversationSession from stateCoordinatorMap
    const stateCoordinator = stateCoordinatorMap?.get(threadId);
    if (!stateCoordinator) {
      throw new Error(`ThreadStateCoordinator not found for thread: ${threadId}`);
    }
    const conversationManager = stateCoordinator.getConversationManager();
    const result = await conversationManager.executeMessageOperation(operationConfig);

    return createSuccessResult(
      triggerId,
      action,
      {
        message: `Message operation ${operationConfig.operation} applied successfully to thread ${threadId}`,
        stats: result.stats,
      },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
