/**
 * Execute Workflow Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type {
  WorkflowToolExecutionContext,
  ExecuteWorkflowParams,
  ExecuteWorkflowResult,
} from "../../../../../../workflow/execution/types/workflow-tool.types.js";
import type {
  TriggeredSubgraphTask,
  ExecutedSubgraphResult,
  TaskSubmissionResult,
} from "../../../../../../workflow/execution/types/triggered-subworkflow.types.js";
import { getContainer } from "../../../../../../core/di/index.js";
import * as Identifiers from "../../../../../../core/di/service-identifiers.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { TriggeredSubworkflowHandler } from "../../../../../../workflow/execution/handlers/triggered-subworkflow-handler.js";

/**
 * Create execute workflow handler
 */
export function createExecuteWorkflowHandler() {
  return async (
    params: Record<string, unknown>,
    context: BuiltinToolExecutionContext,
  ): Promise<ExecuteWorkflowResult> => {
    const {
      workflowId,
      input = {},
      waitForCompletion = true,
      timeout,
    } = params as unknown as ExecuteWorkflowParams;

    // Cast to WorkflowToolExecutionContext for type safety
    const workflowContext = context as WorkflowToolExecutionContext;

    // Validate required parameters
    if (!workflowId) {
      throw new RuntimeValidationError("workflowId is required for execute_workflow", {
        operation: "execute_workflow",
        field: "workflowId",
        value: params,
        context: {
          threadId: context.threadId,
          hasParentThread: !!context.parentThreadEntity,
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
          operation: "execute_workflow",
          context: {
            workflowId,
            threadId: context.threadId,
            containerInitialized: true,
          },
        },
      );
    }

    // Validate parent thread entity
    if (!workflowContext.parentThreadEntity) {
      throw new RuntimeValidationError("Parent thread entity is required for workflow execution", {
        operation: "execute_workflow",
        field: "parentThreadEntity",
        context: {
          workflowId,
          threadId: workflowContext.threadId,
          availableContextKeys: Object.keys(workflowContext),
        },
      });
    }

    // Build execution task
    const task: TriggeredSubgraphTask = {
      subgraphId: workflowId,
      input,
      mainThreadEntity: workflowContext.parentThreadEntity,
      mainWorkflowExecutionEntity: workflowContext.parentThreadEntity,
      triggerId: `builtin-${Date.now()}`,
      config: {
        waitForCompletion,
        timeout,
      },
    };

    // Execute workflow
    const result = await triggeredSubworkflowManager.executeTriggeredSubgraph(task);

    // Process result
    if ("threadResult" in result) {
      // Synchronous execution completed
      const executedResult = result as ExecutedSubgraphResult;
      return {
        success: true,
        status: "completed",
        output: executedResult.subgraphEntity.getOutput(),
        executionTime: executedResult.executionTime,
      };
    } else {
      // Asynchronous execution submitted
      const submissionResult = result as TaskSubmissionResult;
      return {
        success: true,
        status: "submitted",
        taskId: submissionResult.taskId,
        message: submissionResult.message,
      };
    }
  };
}
