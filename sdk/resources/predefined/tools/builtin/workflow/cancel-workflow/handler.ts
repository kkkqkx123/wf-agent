/**
 * Cancel Workflow Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type {
  CancelWorkflowResult,
} from "../../../../../../workflow/execution/types/workflow-tool.types.js";
import * as Identifiers from "../../../../../../core/di/service-identifiers.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { TriggeredSubworkflowHandler } from "../../../../../../workflow/execution/handlers/triggered-subworkflow-handler.js";
import {
  CancelWorkflowParamsSchema,
  assertWorkflowContext,
} from "../../../../../../workflow/execution/types/workflow-tool.types.js";

/**
 * Cancel Workflow Tool Handler
 * 
 * @param params - CancelWorkflowParams containing taskId
 * @param context - WorkflowToolExecutionContext with parentExecutionEntity and globalContext
 * @returns CancelWorkflowResult with cancellation status
 */
export function createCancelWorkflowHandler() {
  return async (
    params: unknown,
    context: BuiltinToolExecutionContext,
  ): Promise<CancelWorkflowResult> => {
    // Validate parameters using Zod schema
    const validatedParams = CancelWorkflowParamsSchema.parse(params);
    const { taskId } = validatedParams;

    // Validate context using type guard
    assertWorkflowContext(context);
    const workflowContext = context;

    // Validate required parameters
    if (!taskId) {
      throw new RuntimeValidationError("taskId is required for cancel_workflow", {
        operation: "cancel_workflow",
        field: "taskId",
        value: params,
        context: {
          executionId: context.executionId,
        },
      });
    }

    // Validate parent workflow execution entity for hierarchy security
    if (!context.parentExecutionEntity) {
      throw new RuntimeValidationError("Parent workflow execution entity is required for workflow cancellation", {
        operation: "cancel_workflow",
        field: "parentExecutionEntity",
        context: {
          taskId,
          executionId: context.executionId,
        },
      });
    }

    // Get TriggeredSubworkflowHandler from DI container
    const globalContext = workflowContext.globalContext;
    if (!globalContext) {
      throw new RuntimeValidationError("GlobalContext not available", {
        operation: "cancel_workflow",
      });
    }
    
    const triggeredSubworkflowManager = globalContext.container.get(
      Identifiers.TriggeredSubworkflowHandler,
    ) as TriggeredSubworkflowHandler;

    if (!triggeredSubworkflowManager) {
      throw new RuntimeValidationError(
        "TriggeredSubworkflowHandler not available in DI container",
        {
          operation: "cancel_workflow",
          context: {
            taskId,
            executionId: context.executionId,
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
