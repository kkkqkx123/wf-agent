/**
 * Restore Thread Handling Function
 *
 * Responsible for initiating the actions required to restore the thread
 * Coordinates the recovery process through the WorkflowStateTransitor
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

export async function resumeThreadHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    if (action.type !== "resume_thread") {
      throw new RuntimeValidationError("Action type must be resume_thread", {
        operation: "handle",
        field: "type",
      });
    }

    const { executionId } = action.parameters;

    const executionEntity = workflowExecutionRegistry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }

    executionEntity.resume();

    return createSuccessResult(
      triggerId,
      action,
      { message: `Thread ${executionId} resumed successfully` },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
