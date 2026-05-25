import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import { createSuccessResult, createFailureResult } from "./trigger-handler-utils.js";

export async function resumeExecutionHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    const { executionId } = action.parameters as { executionId?: string };

    if (!executionId) {
      throw new RuntimeValidationError("executionId is required for resume_workflow_execution", {
        operation: "handle",
        field: "parameters.executionId",
      });
    }

    const executionEntity = workflowExecutionRegistry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }

    executionEntity.resume();

    const executionTime = diffTimestamp(startTime, now());

    return createSuccessResult(
      triggerId,
      action,
      { message: `Workflow execution ${executionId} resumed successfully` },
      executionTime,
    );
  } catch (error) {
    const executionTime = diffTimestamp(startTime, now());
    return createFailureResult(triggerId, action, error, executionTime);
  }
}