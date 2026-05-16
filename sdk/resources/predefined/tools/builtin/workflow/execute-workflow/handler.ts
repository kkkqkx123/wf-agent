/**
 * Execute Workflow Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type {
  ExecuteWorkflowResult,
} from "@sdk/workflow/execution/types/workflow-tool.types.js";
import type {
  TriggeredSubgraphTask,
  ExecutedSubgraphResult,
  TaskSubmissionResult,
} from "@sdk/workflow/execution/types/triggered-subworkflow.types.js";
import * as Identifiers from "@sdk/core/di/service-identifiers.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { TriggeredSubworkflowHandler } from "@sdk/workflow/execution/handlers/triggered-subworkflow-handler.js";
import {
  ExecuteWorkflowParamsSchema,
  assertWorkflowContext,
} from "@sdk/workflow/execution/types/workflow-tool.types.js";

/**
 * Execute Workflow Tool Handler
 * 
 * @param params - ExecuteWorkflowParams containing workflowId, input, messageContexts, etc.
 * @param context - WorkflowToolExecutionContext with parentExecutionEntity and globalContext
 * @returns ExecuteWorkflowResult with execution status (COMPLETED or SUBMITTED)
 */
export function createExecuteWorkflowHandler() {
  return async (
    params: unknown,
    context: BuiltinToolExecutionContext,
  ): Promise<ExecuteWorkflowResult> => {
    // Validate parameters using Zod schema
    const validatedParams = ExecuteWorkflowParamsSchema.parse(params);
    const { workflowId, input, messageContexts, waitForCompletion, timeout } = validatedParams;

    // Validate context using type guard
    assertWorkflowContext(context);
    const workflowContext = context;

    // Validate required parameters
    if (!workflowId) {
      throw new RuntimeValidationError("workflowId is required for execute_workflow", {
        operation: "execute_workflow",
        field: "workflowId",
        value: params,
        context: {
          executionId: context.executionId,
          hasParentExecution: !!context.parentExecutionEntity,
        },
      });
    }

    // Validate parent workflow execution entity BEFORE accessing DI container
    if (!workflowContext.parentExecutionEntity) {
      throw new RuntimeValidationError(
        "Parent workflow execution entity is required for workflow execution",
        {
          operation: "execute_workflow",
          field: "parentExecutionEntity",
          context: {
            workflowId,
            executionId: context.executionId,
          },
        },
      );
    }

    // Get TriggeredSubworkflowHandler from DI container
    const globalContext = workflowContext.globalContext;
    if (!globalContext) {
      throw new RuntimeValidationError(
        "GlobalContext not available in execution context",
        {
          operation: "execute_workflow",
          context: {
            workflowId,
            executionId: context.executionId,
          },
        },
      );
    }
    
    const triggeredSubworkflowManager = globalContext.container.get(
      Identifiers.TriggeredSubworkflowHandler,
    ) as TriggeredSubworkflowHandler;

    if (!triggeredSubworkflowManager) {
      throw new RuntimeValidationError(
        "TriggeredSubworkflowHandler not available in DI container",
        {
          operation: "execute_workflow",
          context: {
            workflowId,
            executionId: context.executionId,
            containerInitialized: true,
          },
        },
      );
    }

    // Build execution task
    const task: TriggeredSubgraphTask = {
      subgraphId: workflowId,
      input,
      messageContexts,  // Pass message contexts to the triggered subworkflow
      mainWorkflowExecutionEntity: workflowContext.parentExecutionEntity,
      triggerId: `builtin-${context.executionId}-${Date.now()}`,
      config: {
        triggeredWorkflowId: workflowId,
        waitForCompletion,
        timeout,
      },
    };

    // Execute workflow
    const result = await triggeredSubworkflowManager.executeTriggeredSubgraph(task);

    // Process result
    if ("workflowExecutionResult" in result) {
      // Synchronous execution completed
      const executedResult = result as ExecutedSubgraphResult;
      return {
        success: true,
        status: "COMPLETED",
        output: executedResult.subgraphEntity.getOutput(),
        executionTime: executedResult.executionTime,
      };
    } else {
      // Asynchronous execution submitted
      const submissionResult = result as TaskSubmissionResult;
      return {
        success: true,
        status: "SUBMITTED",
        taskId: submissionResult.taskId,
        message: submissionResult.message,
      };
    }
  };
}
