/**
 * Execute the trigger sub-workflow processing function
 * Responsible for running the isolated sub-workflows triggered by the trigger
 *
 * Responsibilities:
 * - Trigger the execution of the sub-workflow
 * - Pass in input data related to the trigger event
 * - Support both synchronous and asynchronous execution modes
 * - Manage execution using a task queue and thread pool
 *
 * Note:
 * - Data transfer (variables, conversation history) is handled by the node processor
 * - The START_FROM_TRIGGER node is responsible for receiving input data
 * - The CONTINUE_FROM_TRIGGER node is responsible for sending data back to the main thread
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import type { ExecuteTriggeredSubgraphActionConfig } from "@wf-agent/types";
import {
  RuntimeValidationError,
  ThreadContextNotFoundError,
  WorkflowNotFoundError,
} from "@wf-agent/types";
import type { ThreadRegistry } from "../../../stores/thread-registry.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { ThreadBuilder } from "../../factories/thread-builder.js";
import type { TaskQueue } from "../../../stores/task/task-queue.js";
import { getErrorMessage, now, diffTimestamp } from "@wf-agent/common-utils";
import type { TriggeredSubgraphTask } from "../../types/triggered-subworkflow.types.js";
import { getContainer } from "../../../../core/di/index.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import type { GraphRegistry } from "../../../stores/graph-registry.js";
import type { WorkflowGraphRegistry } from "../../../stores/workflow-graph-registry.js";
import type { TriggeredSubworkflowHandler } from "../triggered-subworkflow-handler.js";

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
  threadRegistry: ThreadRegistry,
  eventManager: EventRegistry,
  threadBuilder: ThreadBuilder,
  taskQueueManager: TaskQueue,
  currentThreadId?: string,
): Promise<TriggerExecutionResult> {
  const startTime = now();

  try {
    const parameters = action.parameters as ExecuteTriggeredSubgraphActionConfig;
    const { triggeredWorkflowId, waitForCompletion = true } = parameters;
    const timeout = parameters.timeout;
    const recordHistory = parameters.recordHistory;

    if (!triggeredWorkflowId) {
      throw new RuntimeValidationError("Missing required parameter: triggeredWorkflowId", {
        operation: "handle",
        field: "triggeredWorkflowId",
      });
    }

    const threadId = currentThreadId;

    if (!threadId) {
      throw new ThreadContextNotFoundError("Current thread ID not provided", "current");
    }

    const mainThreadEntity = threadRegistry.get(threadId);

    if (!mainThreadEntity) {
      throw new ThreadContextNotFoundError(`Main thread entity not found: ${threadId}`, threadId);
    }

    const container = getContainer();
    const graphRegistry = container.get(Identifiers.GraphRegistry) as WorkflowGraphRegistry;
    const processedTriggeredWorkflow = graphRegistry.get(triggeredWorkflowId);

    if (!processedTriggeredWorkflow) {
      throw new WorkflowNotFoundError(
        `Triggered workflow not found or not preprocessed: ${triggeredWorkflowId}`,
        triggeredWorkflowId,
      );
    }

    const input: Record<string, unknown> = {
      triggerId,
      output: mainThreadEntity.getOutput(),
      input: mainThreadEntity.getInput(),
    };

    const manager = container.get(
      Identifiers.TriggeredSubworkflowHandler,
    ) as TriggeredSubworkflowHandler;

    const task: TriggeredSubgraphTask = {
      subgraphId: triggeredWorkflowId,
      input,
      triggerId,
      mainThreadEntity,
      config: {
        waitForCompletion,
        timeout,
        recordHistory,
      },
    };

    const result = await manager.executeTriggeredSubgraph(task);

    const executionTime = diffTimestamp(startTime, now());

    if (waitForCompletion) {
      const syncResult = result as {
        subgraphEntity: { getOutput: () => Record<string, unknown> };
        executionTime: number;
      };
      return createSyncSuccessResult(
        triggerId,
        action,
        {
          triggeredWorkflowId,
          input,
          output: syncResult.subgraphEntity.getOutput(),
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
