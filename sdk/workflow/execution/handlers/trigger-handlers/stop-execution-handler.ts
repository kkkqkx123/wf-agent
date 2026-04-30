/**
 * Workflow execution termination function
 *
 * Responsible for initiating the action to stop a workflow execution
 * Coordinates the termination process through the WorkflowStateTransitor, which includes cascading the cancellation of child workflow executions
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
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

export async function stopExecutionHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    if (action.type !== "stop_workflow_execution") {
      throw new RuntimeValidationError("Action type must be stop_workflow_execution", {
        operation: "handle",
        field: "type",
      });
    }

    const { executionId } = action.parameters;

    const executionEntity = workflowExecutionRegistry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }

    executionEntity.stop();

    return createSuccessResult(
      triggerId,
      action,
      { message: `Workflow execution ${executionId} stopped successfully` },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
