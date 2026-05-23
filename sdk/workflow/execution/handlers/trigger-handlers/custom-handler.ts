/**
 * Graph Custom Action Handling Function
 *
 * Implement custom action handling specific to Graph based on the general framework of sdk/core/triggers.
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { BaseTriggerDefinition } from "../../../../core/triggers/index.js";
import { getErrorMessage, now } from "@wf-agent/common-utils";
import { ContainerManager } from "../../../../core/di/container-manager.js";
import * as ServiceIdentifiers from "../../../../core/di/service-identifiers.js";

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
  containerId: string,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    if (action.type !== "custom") {
      throw new RuntimeValidationError("Action type must be custom", {
        operation: "handle",
        field: "type",
      });
    }

    const { handlerName, data } = action.parameters;

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

    const container = ContainerManager.getInstance().getContainer(containerId);
    const registry = container.get(ServiceIdentifiers.CustomHandlerRegistry);
    const handler = registry.getHandler(handlerName);

    if (!handler) {
      const availableHandlers = registry.getRegisteredNames();
      return createFailureResult(
        triggerId,
        action,
        `Custom handler '${handlerName}' not found. Available handlers: [${availableHandlers.join(", ")}]`,
        startTime,
      );
    }

    const result = await handler(trigger, (data as Record<string, unknown>) || {});

    if (result.success) {
      return createSuccessResult(
        triggerId,
        action,
        { message: "Custom action executed successfully", result: result.result },
        now() - startTime,
      );
    } else {
      return createFailureResult(triggerId, action, result.error, now() - startTime);
    }
  } catch (error) {
    return createFailureResult(triggerId, action, error, startTime);
  }
}
