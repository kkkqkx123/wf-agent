import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import type { NodeExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import { buildNodeCompletedEvent } from "../../utils/event/index.js";
import { createSuccessResult, createFailureResult } from "./trigger-handler-utils.js";

export async function skipNodeHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  eventManager: EventRegistry,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    const { executionId, nodeId } = action.parameters as { executionId?: string; nodeId?: string };

    if (!executionId) {
      throw new RuntimeValidationError("executionId is required for skip_node", {
        operation: "handle",
        field: "parameters.executionId",
      });
    }

    if (!nodeId) {
      throw new RuntimeValidationError("nodeId is required for skip_node", {
        operation: "handle",
        field: "parameters.nodeId",
      });
    }

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

    const executionTime = diffTimestamp(startTime, now());

    return createSuccessResult(
      triggerId,
      action,
      { message: `Node ${nodeId} skipped successfully in workflow execution ${executionId}` },
      executionTime,
    );
  } catch (error) {
    const executionTime = diffTimestamp(startTime, now());
    return createFailureResult(triggerId, action, error, executionTime);
  }
}