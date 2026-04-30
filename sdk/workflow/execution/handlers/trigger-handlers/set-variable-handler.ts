/**
 * Set up a variable handling function
 * Responsible for executing the actions that trigger the setting of variables
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";

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
    error: error instanceof Error ? error.message : String(error),
  };
}

export async function setVariableHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    if (action.type !== "set_variable") {
      throw new RuntimeValidationError("Action type must be set_variable", {
        operation: "handle",
        field: "type",
      });
    }

    const { executionId, variables } = action.parameters;

    const executionEntity = workflowExecutionRegistry.get(executionId);

    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found: ${executionId}`, executionId);
    }

    for (const [name, value] of Object.entries(variables)) {
      executionEntity.setVariable(name, value);
    }

    const executionTime = diffTimestamp(startTime, now());

    return createSuccessResult(
      triggerId,
      action,
      { message: `Variables updated successfully in workflow execution ${executionId}`, variables },
      executionTime,
    );
  } catch (error) {
    const executionTime = diffTimestamp(startTime, now());
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
