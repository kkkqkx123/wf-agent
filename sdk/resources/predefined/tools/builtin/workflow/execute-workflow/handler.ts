/**
 * Execute Workflow Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type {
  ExecuteWorkflowResult,
} from "@sdk/workflow/execution/types/workflow-tool.types.js";
import type {
  TriggeredSubworkflowTask,
  ExecutedSubworkflowResult,
  TaskSubmissionResult,
} from "@sdk/workflow/execution/types/triggered-subworkflow.types.js";
import * as Identifiers from "@sdk/core/di/service-identifiers.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { TriggeredSubworkflowHandler } from "@sdk/workflow/execution/handlers/triggered-subworkflow-handler.js";
import {
  ExecuteWorkflowParamsSchema,
  assertWorkflowContext,
} from "@sdk/workflow/execution/types/workflow-tool.types.js";
import type { WorkflowHandlerConfig } from "../types.js";
import { formatAvailableWorkflows } from "../types.js";

/**
 * Execute Workflow Tool Handler
 * 
 * @param config - Optional WorkflowHandlerConfig for workflow visibility control.
 *                 When provided, validates workflowId against available workflows.
 *                 When not provided, allows any workflowId (backward compatible).
 * @returns Handler function for execute_workflow tool
 */
export function createExecuteWorkflowHandler(config?: WorkflowHandlerConfig) {
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

    // Validate workflowId against available workflows (when config is provided)
    if (config && !config.loader.hasWorkflow(workflowId)) {
      const available = config.loader.getAvailableWorkflows();
      throw new RuntimeValidationError(
        `Workflow '${workflowId}' not found.\n\nAvailable workflows:\n${formatAvailableWorkflows(available)}\n\n` +
        `Use the 'execute_workflow' tool with one of the available workflow IDs listed above.`,
        {
          operation: "execute_workflow",
          field: "workflowId",
          value: workflowId,
          context: {
            executionId: context.executionId,
            availableWorkflowIds: available.map(w => w.id),
          },
        },
      );
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
    const task: TriggeredSubworkflowTask = {
      subworkflowId: workflowId,
      input,
      mainWorkflowExecutionEntity: workflowContext.parentExecutionEntity,
      triggerId: `builtin-${context.executionId}-${Date.now()}`,
      sourceType: 'workflow',
      config: {
        triggeredWorkflowId: workflowId,
        waitForCompletion,
        timeout,
        // Pass message contexts via inputMapping.additionalParams if provided
        ...(messageContexts && {
          inputMapping: {
            additionalParams: {
              messageContexts,
            },
          },
        }),
      },
    };

    // Execute workflow
    const result = await triggeredSubworkflowManager.executeTriggeredSubgraph(task);

    // Process result
    if ("workflowExecutionResult" in result) {
      // Synchronous execution completed
      const executedResult = result as ExecutedSubworkflowResult;
      return {
        success: true,
        status: "COMPLETED",
        output: executedResult.subworkflowEntity.getOutput(),
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
