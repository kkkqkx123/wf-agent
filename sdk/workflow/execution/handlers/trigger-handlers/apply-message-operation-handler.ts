/**
 * Application message operation processing function
 *
 * Responsible for executing application message operations (such as context compression, message truncation, etc.)
 * Obtains the ConversationSession of the workflow execution through WorkflowExecutionStateCoordinator to perform the operations
 */

import type {
  TriggerAction,
  TriggerExecutionResult,
  MessageOperationConfig,
} from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import type { WorkflowStateCoordinator } from "../../../state-managers/workflow-state-coordinator.js";
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
 * @param executionRegistry WorkflowExecution registry
 * @param stateCoordinatorMap Optional map of WorkflowExecutionStateCoordinator by executionId
 */
export async function applyMessageOperationHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  stateCoordinatorMap?: Map<string, WorkflowStateCoordinator>,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    const { executionId, operationConfig } = action.parameters as {
      executionId?: string;
      operationConfig?: MessageOperationConfig;
    };

    if (!executionId) {
      throw new RuntimeValidationError("executionId is required for APPLY_MESSAGE_OPERATION action", {
        operation: "handle",
        field: "parameters.executionId",
      });
    }

    if (!operationConfig) {
      throw new RuntimeValidationError(
        "operationConfig is required for APPLY_MESSAGE_OPERATION action",
        { operation: "handle", field: "parameters.operationConfig" },
      );
    }

    const workflowExecutionEntity = workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new Error(`Workflow execution not found: ${executionId}`);
    }

    // Get ConversationSession from stateCoordinatorMap
    const stateCoordinator = stateCoordinatorMap?.get(executionId);
    if (!stateCoordinator) {
      throw new Error(`WorkflowExecutionStateCoordinator not found for workflow execution: ${executionId}`);
    }
    const conversationManager = stateCoordinator.getConversationManager();
    const result = await conversationManager.executeMessageOperation(operationConfig);

    return createSuccessResult(
      triggerId,
      action,
      {
        message: `Message operation ${operationConfig.operation} applied successfully to thread ${executionId}`,
        stats: result.stats,
      },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
