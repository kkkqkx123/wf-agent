import type {
  TriggerAction,
  TriggerExecutionResult,
  MessageOperationConfig,
} from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../registry/workflow-execution-registry.js";
import type { WorkflowStateCoordinator } from "../../../state-managers/workflow-state-coordinator.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import { createSuccessResult, createFailureResult } from "./trigger-handler-utils.js";

export async function applyMessageOperationHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  stateCoordinatorMap?: Map<string, WorkflowStateCoordinator>,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    const { executionId, operationConfig } = action.parameters as {
      executionId?: string;
      operationConfig?: MessageOperationConfig;
    };

    if (!executionId) {
      throw new RuntimeValidationError(
        "executionId is required for APPLY_MESSAGE_OPERATION action",
        {
          operation: "handle",
          field: "parameters.executionId",
        },
      );
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

    const stateCoordinator = stateCoordinatorMap?.get(executionId);
    if (!stateCoordinator) {
      throw new Error(`WorkflowStateCoordinator not found for workflow execution: ${executionId}`);
    }
    const conversationManager = stateCoordinator.getConversationManager();
    const result = await conversationManager.executeMessageOperation(operationConfig);

    const executionTime = diffTimestamp(startTime, now());

    return createSuccessResult(
      triggerId,
      action,
      {
        message: `Message operation ${operationConfig.operation} applied successfully to workflow execution ${executionId}`,
        stats: result.stats,
      },
      executionTime,
    );
  } catch (error) {
    const executionTime = diffTimestamp(startTime, now());
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
