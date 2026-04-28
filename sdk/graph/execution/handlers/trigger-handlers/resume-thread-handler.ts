/**
 * Restore Thread Handling Function
 *
 * Responsible for initiating the actions required to restore the thread
 * Coordinates the recovery process through the ThreadStateTransitor
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

export async function resumeThreadHandler(
  action: TriggerAction,
  triggerId: string,
  stateTransitor: ThreadStateTransitor,
  threadRegistry: ThreadRegistry,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    if (action.type !== "resume_thread") {
      throw new RuntimeValidationError("Action type must be resume_thread", {
        operation: "handle",
        field: "type",
      });
    }

    const { threadId } = action.parameters;

    const threadEntity = threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`Thread not found: ${threadId}`, threadId);
    }

    await stateTransitor.resumeThread(threadEntity);

    return createSuccessResult(
      triggerId,
      action,
      { message: `Thread ${threadId} resumed successfully` },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
