import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import { createSuccessResult, createFailureResult } from "./trigger-handler-utils.js";

export async function setVariableHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    const { executionId, variables } = action.parameters as {
      executionId?: string;
      variables?: Record<string, unknown>;
    };

    if (!executionId) {
      throw new RuntimeValidationError("executionId is required for set_variable", {
        operation: "handle",
        field: "parameters.executionId",
      });
    }

    if (!variables) {
      throw new RuntimeValidationError("variables is required for set_variable", {
        operation: "handle",
        field: "parameters.variables",
      });
    }

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