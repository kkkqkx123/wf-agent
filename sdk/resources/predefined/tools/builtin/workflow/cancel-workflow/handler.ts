/**
 * Cancel Workflow Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type {
  CancelWorkflowParams,
  CancelWorkflowResult,
} from "../../../../../../graph/execution/types/workflow-tool.types.js";
import { getContainer } from "../../../../../../core/di/index.js";
import * as Identifiers from "../../../../../../core/di/service-identifiers.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { TriggeredSubworkflowHandler } from "../../../../../../graph/execution/handlers/triggered-subworkflow-handler.js";

/**
 * Create cancel workflow handler
 */
export function createCancelWorkflowHandler() {
  return async (
    params: Record<string, unknown>,
    context: BuiltinToolExecutionContext,
  ): Promise<CancelWorkflowResult> => {
    const { taskId } = params as unknown as CancelWorkflowParams;

    // Validate required parameters
    if (!taskId) {
      throw new RuntimeValidationError("taskId is required for cancel_workflow", {
        operation: "cancel_workflow",
        field: "taskId",
        value: params,
        context: {
          threadId: context.threadId,
        },
      });
    }

    // Get TriggeredSubworkflowHandler from DI container
    const container = getContainer();
    const triggeredSubworkflowManager = container.get(
      Identifiers.TriggeredSubworkflowHandler,
    ) as TriggeredSubworkflowHandler;

    if (!triggeredSubworkflowManager) {
      throw new RuntimeValidationError(
        "TriggeredSubworkflowHandler not available in DI container",
        {
          operation: "cancel_workflow",
          context: {
            taskId,
            threadId: context.threadId,
            containerInitialized: true,
          },
        },
      );
    }

    // Cancel task
    const success = await triggeredSubworkflowManager.cancelTask(taskId);

    return {
      success,
      taskId,
      message: success ? "Task cancelled successfully" : "Failed to cancel task",
    };
  };
}
