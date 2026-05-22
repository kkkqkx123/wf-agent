/**
 * Execute the trigger sub-workflow processing function
 * Responsible for running the isolated sub-workflows triggered by the trigger
 *
 * Responsibilities:
 * - Trigger the execution of the sub-workflow
 * - Pass in input data related to the trigger event
 * - Support both synchronous and asynchronous execution modes
 * - Manage execution using a task queue and workflow execution pool
 *
 * Note:
 * - Data transfer (variables, conversation history) is handled by the node processor
 * - The START_FROM_TRIGGER node is responsible for receiving input data
 * - The CONTINUE_FROM_TRIGGER node is responsible for sending data back to the main workflow execution
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import type { ExecuteTriggeredSubworkflowActionConfig } from "@wf-agent/types";
import {
  RuntimeValidationError,
  WorkflowExecutionNotFoundError,
  WorkflowNotFoundError,
} from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import { getErrorMessage, now, diffTimestamp } from "@wf-agent/common-utils";
import type { TriggeredSubworkflowTask } from "../../types/triggered-subworkflow.types.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import type { GlobalContext } from "../../../../core/global-context.js";
import type { WorkflowGraphRegistry } from "../../../stores/workflow-graph-registry.js";
import type { TriggeredSubworkflowHandler } from "../triggered-subworkflow-handler.js";
import type { AgentLoopEntity } from "../../../../agent/entities/agent-loop-entity.js";

function createSyncSuccessResult(
  triggerId: string,
  action: TriggerAction,
  data: {
    triggeredWorkflowId: string;
    input: Record<string, unknown>;
    output: Record<string, unknown>;
    executionTime: number;
  },
  executionTime: number,
): TriggerExecutionResult {
  return {
    triggerId,
    success: true,
    action,
    executionTime,
    result: {
      message: `Triggered subgraph execution completed: ${data.triggeredWorkflowId}`,
      triggeredWorkflowId: data.triggeredWorkflowId,
      input: data.input,
      output: data.output,
      waitForCompletion: true,
      executed: true,
      completed: true,
      executionTime: data.executionTime,
    },
  };
}

function createAsyncSuccessResult(
  triggerId: string,
  action: TriggerAction,
  data: {
    triggeredWorkflowId: string;
    taskId: string;
    status: string;
    executionTime: number;
  },
  executionTime: number,
): TriggerExecutionResult {
  return {
    triggerId,
    success: true,
    action,
    executionTime,
    result: {
      message: `Triggered subgraph submitted: ${data.triggeredWorkflowId}`,
      triggeredWorkflowId: data.triggeredWorkflowId,
      taskId: data.taskId,
      status: data.status,
      waitForCompletion: false,
      executed: true,
      completed: false,
      executionTime: data.executionTime,
    },
  };
}

function createFailureResult(
  triggerId: string,
  action: TriggerAction,
  error: unknown,
  executionTime: number,
): TriggerExecutionResult {
  return {
    triggerId,
    success: false,
    action,
    executionTime,
    error: getErrorMessage(error),
  };
}

export async function executeTriggeredSubgraphHandler(
  action: TriggerAction,
  triggerId: string,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  currentExecutionId?: string,
  globalContext?: GlobalContext,
  agentLoopEntity?: AgentLoopEntity,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    const parameters = action.parameters as ExecuteTriggeredSubworkflowActionConfig;
    const { triggeredWorkflowId, waitForCompletion = true } = parameters;

    if (!triggeredWorkflowId) {
      throw new RuntimeValidationError("Missing required parameter: triggeredWorkflowId", {
        operation: "handle",
        field: "triggeredWorkflowId",
      });
    }

    const executionId = currentExecutionId;

    if (!executionId) {
      throw new WorkflowExecutionNotFoundError("Current execution ID not provided", "current");
    }

    const mainWorkflowExecutionEntity = workflowExecutionRegistry.get(executionId);

    if (!mainWorkflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`Main workflow execution entity not found: ${executionId}`, executionId);
    }

    if (!globalContext) {
      throw new RuntimeValidationError("GlobalContext is required for execute_triggered_subgraph", {
        operation: "handle",
        field: "globalContext",
      });
    }

    const container = globalContext.container;
    const graphRegistry = container.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;
    const processedTriggeredWorkflow = graphRegistry.get(triggeredWorkflowId);

    if (!processedTriggeredWorkflow) {
      throw new WorkflowNotFoundError(
        `Triggered workflow not found or not preprocessed: ${triggeredWorkflowId}`,
        triggeredWorkflowId,
      );
    }

    const manager = container.get(
      Identifiers.TriggeredSubworkflowHandler,
    ) as TriggeredSubworkflowHandler;

    const task: TriggeredSubworkflowTask = {
      subworkflowId: triggeredWorkflowId,
      input: { triggerId },
      triggerId,
      mainWorkflowExecutionEntity,
      config: parameters,
      sourceType: agentLoopEntity ? 'agent' : 'workflow',
      sourceEntityId: agentLoopEntity?.id,
    };

    const result = await manager.executeTriggeredSubgraph(task);

    const executionTime = diffTimestamp(startTime, now());

    if (waitForCompletion) {
      const syncResult = result as {
        subworkflowEntity: { getOutput: () => Record<string, unknown> };
        executionTime: number;
      };
      return createSyncSuccessResult(
        triggerId,
        action,
        {
          triggeredWorkflowId,
          input: task.input,
          output: syncResult.subworkflowEntity.getOutput(),
          executionTime: syncResult.executionTime,
        },
        executionTime,
      );
    } else {
      const asyncResult = result as { taskId: string; status: string };
      return createAsyncSuccessResult(
        triggerId,
        action,
        {
          triggeredWorkflowId,
          taskId: asyncResult.taskId,
          status: asyncResult.status,
          executionTime: diffTimestamp(startTime, now()),
        },
        executionTime,
      );
    }
  } catch (error) {
    const executionTime = diffTimestamp(startTime, now());
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
