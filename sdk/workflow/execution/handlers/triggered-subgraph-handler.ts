/**
 * Trigger Sub-workflow Processing Function
 * Provides a stateless function to execute sub-workflows
 *
 * Responsibilities:
 * - Create the context for the sub-workflow
 * - Trigger the start and completion events of the sub-workflow
 * - Execute the sub-workflow
 * - Manage the execution status of the sub-workflow
 *
 * Design Principles:
 * - Stateless functional design
 * - Each function has a single responsibility
 * - Consistency with other trigger processing functions
 * - Sub-workflows are executed asynchronously to avoid blocking the main workflow
 */

import type { ID, WorkflowExecutionResult } from "@wf-agent/types";
import type { ThreadEntity } from "../../entities/workflow-execution-entity.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { now, diffTimestamp, getErrorMessage, getErrorOrNew } from "@wf-agent/common-utils";
import { createSubgraphMetadata } from "./subgraph-handler.js";
import type {
  TriggeredSubgraphTask,
  ExecutedSubgraphResult,
} from "../types/triggered-subworkflow.types.js";
import {
  buildTriggeredSubgraphStartedEvent,
  buildTriggeredSubgraphCompletedEvent,
  buildTriggeredSubgraphFailedEvent,
} from "../../../core/utils/event/builders/subgraph-events.js";

/**
 * Sub-workflow Executor Interface
 */
export interface SubgraphExecutor {
  executeThread(threadEntity: ThreadEntity): Promise<unknown>;
}

/**
 * Sub-workflow context factory interface
 * Used to create sub-workflow contexts, avoiding direct dependency on ThreadBuilder
 */
export interface SubgraphContextFactory {
  buildSubgraphContext(
    subgraphId: ID,
    input: Record<string, unknown>,
    metadata: Record<string, unknown>,
  ): Promise<ThreadEntity>;
}

/**
 * Create a sub-workflow context
 * @param task: The task that triggers the sub-workflow
 * @param contextFactory: The factory for creating the sub-workflow context
 * @returns: The sub-workflow context
 */
export async function createSubgraphContext(
  task: TriggeredSubgraphTask,
  contextFactory: SubgraphContextFactory,
): Promise<ThreadEntity> {
  const metadata = createSubgraphMetadata(task.triggerId, task.mainThreadEntity.id);

  const subgraphEntity = await contextFactory.buildSubgraphContext(
    task.subgraphId,
    task.input,
    metadata,
  );

  return subgraphEntity;
}

/**
 * Trigger the start event of a sub-workflow
 * @param mainThreadEntity: The main workflow thread entity
 * @param task: The task that triggers the sub-workflow
 * @param eventManager: The event manager
 */
export async function emitSubgraphStartedEvent(
  mainThreadEntity: ThreadEntity,
  task: TriggeredSubgraphTask,
  eventManager: EventRegistry,
): Promise<void> {
  await eventManager.emit(
    buildTriggeredSubgraphStartedEvent({
      threadId: mainThreadEntity.id,
      workflowId: mainThreadEntity.getWorkflowId(),
      subgraphId: task.subgraphId,
      triggerId: task.triggerId,
      input: task.input,
    }),
  );
}

/**
 * Trigger the completion event of a sub-workflow
 * @param mainThreadEntity: The main workflow thread entity
 * @param task: The sub-workflow task that was triggered
 * @param subgraphEntity: The sub-workflow thread entity
 * @param executionTime: Execution time (in milliseconds)
 * @param eventManager: The event manager
 */
export async function emitSubgraphCompletedEvent(
  mainThreadEntity: ThreadEntity,
  task: TriggeredSubgraphTask,
  subgraphEntity: ThreadEntity,
  executionTime: number,
  eventManager: EventRegistry,
): Promise<void> {
  await eventManager.emit(
    buildTriggeredSubgraphCompletedEvent({
      threadId: mainThreadEntity.id,
      workflowId: mainThreadEntity.getWorkflowId(),
      subgraphId: task.subgraphId,
      triggerId: task.triggerId,
      output: subgraphEntity.getOutput(),
      executionTime,
    }),
  );
}

/**
 * Triggered Sub-Workflow Failure Event
 * @param mainThreadEntity: Main workflow thread entity
 * @param task: Sub-workflow task that was triggered
 * @param error: Error message
 * @param executionTime: Execution time (in milliseconds)
 * @param eventManager: Event manager
 */
export async function emitSubgraphFailedEvent(
  mainThreadEntity: ThreadEntity,
  task: TriggeredSubgraphTask,
  error: Error | string,
  executionTime: number,
  eventManager: EventRegistry,
): Promise<void> {
  await eventManager.emit(
    buildTriggeredSubgraphFailedEvent({
      threadId: mainThreadEntity.id,
      workflowId: mainThreadEntity.getWorkflowId(),
      subgraphId: task.subgraphId,
      triggerId: task.triggerId,
      error: error instanceof Error ? error : new Error(getErrorMessage(error)),
      executionTime,
    }),
  );
}

/**
 * Execute a single trigger workflow
 * @param task The trigger workflow task
 * @param contextFactory The sub-workflow context factory
 * @param subgraphExecutor The sub-workflow executor
 * @param eventManager The event manager
 * @returns The execution result, including the sub-workflow context, execution outcome, and execution time
 */
export async function executeSingleTriggeredSubgraph(
  task: TriggeredSubgraphTask,
  contextFactory: SubgraphContextFactory,
  subgraphExecutor: SubgraphExecutor,
  eventManager: EventRegistry,
): Promise<ExecutedSubgraphResult> {
  const startTime = now();

  try {
    // Create a sub-workflow context
    const subgraphEntity = await createSubgraphContext(task, contextFactory);

    // Trigger the start event of the workflow.
    await emitSubgraphStartedEvent(task.mainThreadEntity, task, eventManager);

    // Execute the sub-workflow
    const threadResult = await subgraphExecutor.executeThread(subgraphEntity);

    const executionTime = diffTimestamp(startTime, now());

    // Trigger the completion event of the workflow.
    await emitSubgraphCompletedEvent(
      task.mainThreadEntity,
      task,
      subgraphEntity,
      executionTime,
      eventManager,
    );

    return {
      subgraphEntity,
      threadResult: threadResult as WorkflowExecutionResult,
      executionTime,
    };
  } catch (error) {
    const executionTime = diffTimestamp(startTime, now());

    // Triggered workflow failure event
    await emitSubgraphFailedEvent(
      task.mainThreadEntity,
      task,
      getErrorOrNew(error),
      executionTime,
      eventManager,
    );

    // Rethrow the error so that the caller can handle it.
    throw error;
  }
}
