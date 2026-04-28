import { ExecutionError } from "@wf-agent/types";
import type {
  TriggerHandler,
  BaseTriggerDefinition,
  BaseEventData,
} from "../../../../core/triggers/index.js";
import type { ScriptRegistry } from "../../../../core/registry/script-registry.js";

export const executeScriptHandler: TriggerHandler = async (
  trigger: BaseTriggerDefinition,
  eventData: BaseEventData,
) => {
  const config = trigger.action.parameters as {
    scriptName: string;
    parameters?: Record<string, unknown>;
    timeout?: number;
    ignoreError?: boolean;
    validateExistence?: boolean;
  };

  const scriptService = (eventData.data as { scriptService?: ScriptRegistry })?.scriptService;

  if (!scriptService) {
    throw new ExecutionError("ScriptRegistry not available in event data");
  }

  const startTime = Date.now();

  try {
    const result = await scriptService.execute(config.scriptName, {
      ...config.parameters,
      timeout: config.timeout,
    });

    return {
      triggerId: trigger.id,
      success: true,
      action: trigger.action,
      executionTime: Date.now() - startTime,
      result,
    };
  } catch (error) {
    if (config.ignoreError) {
      return {
        triggerId: trigger.id,
        success: true,
        action: trigger.action,
        executionTime: Date.now() - startTime,
        warning: `Script execution failed but ignored: ${error instanceof Error ? error.message : String(error)}`,
      };
    }

    throw new ExecutionError(
      `Script execution failed: ${error instanceof Error ? error.message : String(error)}`,
      undefined,
      undefined,
      undefined,
      error instanceof Error ? error : new Error(String(error)),
    );
  }
};
