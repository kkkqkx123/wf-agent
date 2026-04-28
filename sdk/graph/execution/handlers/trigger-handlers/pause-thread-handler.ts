/**
 * Thread Suspension Handling Function
 *
 * Responsible for initiating the action to suspend a thread
 * The suspension process is coordinated through the ThreadStateTransitor.
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, ThreadContextNotFoundError } from "@wf-agent/types";
import type { ThreadStateTransitor } from "../../coordinators/thread-state-transitor.js";
import type { ThreadRegistry } from "../../../stores/thread-registry.js";
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

export async function pauseThreadHandler(
  action: TriggerAction,
  triggerId: string,
  stateTransitor: ThreadStateTransitor,
  threadRegistry: ThreadRegistry,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    if (action.type !== "pause_thread") {
      throw new RuntimeValidationError("Action type must be pause_thread", {
        operation: "handle",
        field: "type",
      });
    }

    const { threadId } = action.parameters;

    const threadEntity = threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`Thread not found: ${threadId}`, threadId);
    }

    await stateTransitor.pauseThread(threadEntity);

    return createSuccessResult(
      triggerId,
      action,
      { message: `Thread ${threadId} paused successfully` },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
