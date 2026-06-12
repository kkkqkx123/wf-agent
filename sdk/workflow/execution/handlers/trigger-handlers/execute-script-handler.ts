import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, ExecutionError } from "@wf-agent/types";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import type { GlobalContext } from "../../../../core/global-context.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import type { ScriptRegistry } from "../../../../core/registry/script-registry.js";
import { createSuccessResult, createFailureResult } from "./trigger-handler-utils.js";

export async function executeScriptHandler(
  action: TriggerAction,
  triggerId: string,
  globalContext?: GlobalContext,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    const { scriptName, parameters, timeout } = action.parameters as {
      scriptName?: string;
      parameters?: Record<string, unknown>;
      timeout?: number;
      ignoreError?: boolean;
      validateExistence?: boolean;
    };

    if (!scriptName) {
      throw new RuntimeValidationError("scriptName is required for execute_script", {
        operation: "handle",
        field: "parameters.scriptName",
      });
    }

    if (!globalContext) {
      throw new RuntimeValidationError(
        "GlobalContext is required for execute_script. Ensure the SDK is initialized.",
        { operation: "handle", field: "globalContext" },
      );
    }

    const scriptService = globalContext.container.get(Identifiers.ScriptRegistry) as ScriptRegistry;

    if (!scriptService) {
      throw new ExecutionError("ScriptRegistry not available in DI container");
    }

    const result = await scriptService.execute(scriptName, {
      ...parameters,
      timeout,
    });

    const executionTime = diffTimestamp(startTime, now());

    return createSuccessResult(
      triggerId,
      action,
      {
        message: `Script ${scriptName} executed successfully`,
        result,
      },
      executionTime,
    );
  } catch (error) {
    const { ignoreError } = action.parameters as { ignoreError?: boolean };

    if (ignoreError) {
      const executionTime = diffTimestamp(startTime, now());
      return createSuccessResult(
        triggerId,
        action,
        {
          warning: `Script execution failed but ignored: ${error instanceof Error ? error.message : String(error)}`,
        },
        executionTime,
      );
    }

    const executionTime = diffTimestamp(startTime, now());
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
