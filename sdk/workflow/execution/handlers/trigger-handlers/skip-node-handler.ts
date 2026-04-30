/**
 * Skip node processing function
 * Responsible for executing the actions that trigger node skipping
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import type { NodeExecutionResult } from "@wf-agent/types";
import { ValidationError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import { getErrorMessage, now } from "@wf-agent/common-utils";
import { buildNodeCompletedEvent } from "../../utils/event/index.js";

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

export async function skipNodeHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  eventManager: EventRegistry,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    if (action.type !== "skip_node") {
      throw new ValidationError("Action type must be skip_node", "type");
    }

    const { executionId, nodeId } = action.parameters;

    const executionEntity = workflowExecutionRegistry.get(executionId);

    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found: ${executionId}`, executionId);
    }

    const workflowExecution = executionEntity.getExecution();

    const result: NodeExecutionResult = {
      nodeId,
      nodeType: "UNKNOWN",
      status: "SKIPPED",
      step: workflowExecution.nodeResults.length + 1,
      executionTime: 0,
    };

    workflowExecution.nodeResults.push(result);

    const completedEvent = buildNodeCompletedEvent({
      executionId: executionEntity.id,
      workflowId: executionEntity.getWorkflowId(),
      nodeId,
      output: null,
      executionTime: 0,
    });
    await eventManager.emit(completedEvent);

    return createSuccessResult(
      triggerId,
      action,
      { message: `Node ${nodeId} skipped successfully in workflow execution ${executionId}` },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
