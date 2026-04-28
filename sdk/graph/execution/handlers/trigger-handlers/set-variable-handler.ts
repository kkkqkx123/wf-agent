/**
 * Set up a variable handling function
 * Responsible for executing the actions that trigger the setting of variables
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, ThreadContextNotFoundError } from "@wf-agent/types";
import type { ThreadRegistry } from "../../../stores/thread-registry.js";
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
  threadRegistry: ThreadRegistry,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    if (action.type !== "set_variable") {
      throw new RuntimeValidationError("Action type must be set_variable", {
        operation: "handle",
        field: "type",
      });
    }

    const { threadId, variables } = action.parameters;

    const threadEntity = threadRegistry.get(threadId);

    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`ThreadEntity not found: ${threadId}`, threadId);
    }

    for (const [name, value] of Object.entries(variables)) {
      threadEntity.setVariable(name, value);
    }

    const executionTime = diffTimestamp(startTime, now());

    return createSuccessResult(
      triggerId,
      action,
      { message: `Variables updated successfully in thread ${threadId}`, variables },
      executionTime,
    );
  } catch (error) {
    const executionTime = diffTimestamp(startTime, now());
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
