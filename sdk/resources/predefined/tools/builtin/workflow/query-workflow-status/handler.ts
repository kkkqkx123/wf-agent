/**
 * Query Workflow Status Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type {
  QueryWorkflowStatusParams,
  QueryWorkflowStatusResult,
} from "../../../../../../graph/execution/types/workflow-tool.types.js";
import { getContainer } from "../../../../../../core/di/index.js";
import * as Identifiers from "../../../../../../core/di/service-identifiers.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { TriggeredSubworkflowHandler } from "../../../../../../graph/execution/handlers/triggered-subworkflow-handler.js";
import { isThreadInstance } from "../../../../../../core/types/index.js";

/**
 * Create query workflow status handler
 */
export function createQueryWorkflowStatusHandler() {
  return async (
    params: Record<string, unknown>,
    context: BuiltinToolExecutionContext,
  ): Promise<QueryWorkflowStatusResult> => {
    const { taskId } = params as unknown as QueryWorkflowStatusParams;

    // Validate required parameters
    if (!taskId) {
      throw new RuntimeValidationError("taskId is required for query_workflow_status", {
        operation: "query_workflow_status",
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
          operation: "query_workflow_status",
          context: {
            taskId,
            threadId: context.threadId,
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
        status: "not_found",
        message: `Task with ID '${taskId}' not found`,
      };
    }

    // Get instance from taskInfo
    const instance = taskInfo.instance;

    // Check if instance is a ThreadEntity
    if (!isThreadInstance(instance)) {
      return {
        success: false,
        status: "error",
        message: "Task instance is not a thread",
      };
    }

    return {
      success: true,
      status: taskInfo.status,
      threadId: instance.id,
      workflowId: instance.getWorkflowId(),
      createdAt: taskInfo.submitTime,
      updatedAt: taskInfo.completeTime ?? taskInfo.submitTime,
    };
  };
}
