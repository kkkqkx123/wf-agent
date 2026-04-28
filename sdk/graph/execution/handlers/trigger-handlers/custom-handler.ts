/**
 * Graph Custom Action Handling Function
 *
 * Implement custom action handling specific to Graph based on the general framework of sdk/core/triggers.
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import {
  executeCustomAction,
  type BaseTriggerDefinition,
} from "../../../../core/triggers/index.js";
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

export async function customHandler(
  action: TriggerAction,
  triggerId: string,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    if (action.type !== "custom") {
      throw new RuntimeValidationError("Action type must be custom", {
        operation: "handle",
        field: "type",
      });
    }

    const { handlerName, data: _data } = action.parameters;

    if (!handlerName || typeof handlerName !== "string") {
      throw new RuntimeValidationError(
        "handlerName is required and must be a string for CUSTOM action",
        { operation: "handle", field: "parameters.handlerName" },
      );
    }

    const trigger: BaseTriggerDefinition = {
      id: triggerId,
      name: "custom_trigger",
      condition: { eventType: "custom" },
      action: {
        type: "custom",
        parameters: action.parameters as unknown as Record<string, unknown>,
      },
    };

    const result = await executeCustomAction(trigger);

    if (result.success) {
      return createSuccessResult(
        triggerId,
        action,
        { message: "Custom action executed successfully", result: result.result },
        startTime,
      );
    } else {
      return createFailureResult(triggerId, action, result.error, startTime);
    }
  } catch (error) {
    return createFailureResult(triggerId, action, error, startTime);
  }
}
