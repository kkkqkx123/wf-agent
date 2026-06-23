/**
 * Query Workflow Status Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type { QueryWorkflowStatusResult } from "@sdk/workflow/execution/types/workflow-tool.types.js";
import * as Identifiers from "@sdk/di/service-identifiers.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { TriggeredSubworkflowHandler } from "@sdk/workflow/execution/handlers/triggered-subworkflow-handler.js";
import { isWorkflowExecutionInstance } from "@sdk/shared/types/index.js";
import {
  QueryWorkflowStatusParamsSchema,
  assertWorkflowContext,
} from "@sdk/workflow/execution/types/workflow-tool.types.js";

/**
 * Query Workflow Status Tool Handler
 *
 * @param params - QueryWorkflowStatusParams containing taskId
 * @param context - WorkflowToolExecutionContext with parentExecutionEntity and globalContext
 * @returns QueryWorkflowStatusResult with task status information
 */
export function createQueryWorkflowStatusHandler() {
  return async (
    params: unknown,
    context: BuiltinToolExecutionContext,
  ): Promise<QueryWorkflowStatusResult> => {
    // Validate parameters using Zod schema
    const validatedParams = QueryWorkflowStatusParamsSchema.parse(params);
    const { taskId } = validatedParams;

    // Validate context using type guard
    assertWorkflowContext(context);
    const workflowContext = context;

    // Validate required parameters
    if (!taskId) {
      throw new RuntimeValidationError("taskId is required for query_workflow_status", {
        operation: "query_workflow_status",
        field: "taskId",
        value: params,
        context: {
          executionId: context.executionId,
        },
      });
    }

    // Validate parent workflow execution entity for hierarchy security
    if (!context.parentExecutionEntity) {
      throw new RuntimeValidationError(
        "Parent workflow execution entity is required for workflow status query",
        {
          operation: "query_workflow_status",
          field: "parentExecutionEntity",
          context: {
            taskId,
            executionId: context.executionId,
          },
        },
      );
    }

    // Get TriggeredSubworkflowHandler from DI container
    const globalContext = workflowContext.globalContext;
    if (!globalContext) {
      throw new RuntimeValidationError("GlobalContext not available", {
        operation: "query_workflow_status",
      });
    }

    const triggeredSubworkflowManager = globalContext.container.get(
      Identifiers.TriggeredSubworkflowHandler,
    ) as TriggeredSubworkflowHandler;

    if (!triggeredSubworkflowManager) {
      throw new RuntimeValidationError(
        "TriggeredSubworkflowHandler not available in DI container",
        {
          operation: "query_workflow_status",
          context: {
            taskId,
            executionId: context.executionId,
            containerInitialized: true,
          },
        },
      );
    }

    // Query task status
    const taskInfo = triggeredSubworkflowManager.getTaskStatus(taskId);

    if (!taskInfo) {
      return {
        success: false,
        status: "NOT_FOUND",
        message: `Task with ID '${taskId}' not found`,
      };
    }

    // Get instance from taskInfo
    const instance = taskInfo.instance;

    // Check if instance is a WorkflowExecutionEntity
    if (!isWorkflowExecutionInstance(instance)) {
      return {
        success: false,
        status: "ERROR",
        message: "Task instance is not a workflow execution",
      };
    }

    return {
      success: true,
      status: taskInfo.status,
      executionId: instance.id,
      workflowId: instance.getWorkflowId(),
      createdAt: taskInfo.submitTime,
      updatedAt: taskInfo.completeTime ?? taskInfo.submitTime,
    };
  };
}
