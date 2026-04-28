/**
 * Skip node processing function
 * Responsible for executing the actions that trigger node skipping
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import type { NodeExecutionResult } from "@wf-agent/types";
import { ValidationError, ThreadContextNotFoundError } from "@wf-agent/types";
import type { ThreadRegistry } from "../../../stores/thread-registry.js";
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
  threadRegistry: ThreadRegistry,
  eventManager: EventRegistry,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    if (action.type !== "skip_node") {
      throw new ValidationError("Action type must be skip_node", "type");
    }

    const { threadId, nodeId } = action.parameters;

    const threadEntity = threadRegistry.get(threadId);

    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`ThreadEntity not found: ${threadId}`, threadId);
    }

    const thread = threadEntity.getThread();

    const result: NodeExecutionResult = {
      nodeId,
      nodeType: "UNKNOWN",
      status: "SKIPPED",
      step: thread.nodeResults.length + 1,
      executionTime: 0,
    };

    thread.nodeResults.push(result);

    const completedEvent = buildNodeCompletedEvent({
      threadId: threadEntity.id,
      workflowId: threadEntity.getWorkflowId(),
      nodeId,
      output: null,
      executionTime: 0,
    });
    await eventManager.emit(completedEvent);

    return createSuccessResult(
      triggerId,
      action,
      { message: `Node ${nodeId} skipped successfully in thread ${threadId}` },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
