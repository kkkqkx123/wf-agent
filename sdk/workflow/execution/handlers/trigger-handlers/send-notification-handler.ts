import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import { createSuccessResult, createFailureResult } from "./trigger-handler-utils.js";

export async function sendNotificationHandler(
  action: TriggerAction,
  triggerId: string,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    const { message, recipients, level } = action.parameters as {
      message?: string;
      recipients?: string[];
      level?: string;
    };

    if (!message) {
      throw new RuntimeValidationError("message is required for SEND_NOTIFICATION action", {
        operation: "handle",
        field: "parameters.message",
      });
    }

    const executionTime = diffTimestamp(startTime, now());

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
    const executionTime = diffTimestamp(startTime, now());
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
