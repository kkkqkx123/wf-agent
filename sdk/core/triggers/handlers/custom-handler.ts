/**
 * General Custom Action Processor
 *
 * Provides the execution logic for custom trigger actions.
 * Can be reused by the Graph and Agent modules.
 */

import type { BaseTriggerDefinition, TriggerExecutionResult } from "../types.js";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getCustomHandlerRegistry } from "../../registry/custom-handler-registry.js";

const logger = createContextualLogger({ component: "CustomTriggerHandler" });

/**
 * Custom action parameters
 */
export interface CustomActionParameters {
  /** Custom processor name */
  handlerName: string;
  /** Custom parameters */
  data?: Record<string, unknown>;
  /** Other parameters */
  [key: string]: unknown;
}

/**
 * Execute a custom action
 *
 * @param trigger Trigger definition
 * @returns Execution result
 */
export async function executeCustomAction(
  trigger: BaseTriggerDefinition,
): Promise<TriggerExecutionResult> {
  const startTime = now();
  const { action } = trigger;

  logger.debug("Custom trigger action executing", {
    triggerId: trigger.id,
    actionType: action.type,
  });

  try {
    const params = action.parameters as CustomActionParameters;
    const { handlerName } = params;

    if (!handlerName || typeof handlerName !== "string") {
      logger.warn("Custom action missing handlerName", { triggerId: trigger.id });
      return {
        triggerId: trigger.id,
        success: false,
        action,
        executionTime: now() - startTime,
        error: "Custom action requires a handlerName",
      };
    }

    // Look up the registered handler
    const registry = getCustomHandlerRegistry();
    const handler = registry.getHandler(handlerName);

    if (!handler) {
      const availableHandlers = registry.getRegisteredNames();
      logger.warn("Custom handler not found", {
        triggerId: trigger.id,
        handlerName,
        availableHandlers,
      });
      return {
        triggerId: trigger.id,
        success: false,
        action,
        executionTime: now() - startTime,
        error: `Custom handler '${handlerName}' not found. Available handlers: [${availableHandlers.join(", ")}]`,
      };
    }

    // Execute the registered handler
    logger.debug("Executing custom handler", {
      triggerId: trigger.id,
      handlerName,
    });

    const result = await handler(trigger, params.data || {});

    logger.debug("Custom handler executed", {
      triggerId: trigger.id,
      handlerName,
      success: result.success,
    });

    return result;
  } catch (error) {
    logger.error("Custom action execution failed", {
      triggerId: trigger.id,
      error: error instanceof Error ? error.message : String(error),
    });

    return {
      triggerId: trigger.id,
      success: false,
      action,
      executionTime: now() - startTime,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
}
