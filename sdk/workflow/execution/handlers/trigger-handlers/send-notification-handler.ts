/**
 * Send Notification Handling Function
 * Responsible for executing the action that triggers the notification sending
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
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

export async function sendNotificationHandler(
  action: TriggerAction,
  triggerId: string,
): Promise<TriggerExecutionResult> {
  const executionTime = now();

  try {
    if (action.type !== "send_notification") {
      throw new RuntimeValidationError("Action type must be send_notification", {
        operation: "handle",
        field: "type",
      });
    }

    const { message, recipients, level } = action.parameters;

    if (!message) {
      throw new RuntimeValidationError("message is required for SEND_NOTIFICATION action", {
        operation: "handle",
        field: "parameters.message",
      });
    }

    const notificationResult = {
      message,
      recipients: recipients || [],
      level: level || "info",
      timestamp: executionTime,
      status: "sent",
    };

    return createSuccessResult(
      triggerId,
      action,
      { message: "Notification sent successfully", notification: notificationResult },
      executionTime,
    );
  } catch (error) {
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
