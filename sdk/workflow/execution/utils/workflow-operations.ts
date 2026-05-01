/**
 * WorkflowOperations - WorkflowExecution Operation Utility Functions
 * Provides stateless WorkflowExecution operations such as Fork/Join/Copy
 * All functions are pure functions and do not hold any state
 */

import type { WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { WorkflowExecutionBuilder } from "../factories/workflow-execution-builder.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { ExecutionError, RuntimeValidationError } from "@wf-agent/types";
import { MessageArrayUtils } from "../../../core/utils/messages/message-array-utils.js";
import { getErrorMessage, getErrorOrUndefined } from "@wf-agent/common-utils";
import {
  buildWorkflowExecutionForkStartedEvent,
  buildWorkflowExecutionForkCompletedEvent,
  buildWorkflowExecutionJoinStartedEvent,
  buildWorkflowExecutionJoinConditionMetEvent,
  buildWorkflowExecutionCopyStartedEvent,
  buildWorkflowExecutionCopyCompletedEvent,
} from "./event/index.js";
import { emit } from "../../../core/utils/event/event-emitter.js";
import {
  waitForMultipleWorkflowExecutionsCompleted,
  waitForAnyWorkflowExecutionCompleted,
  waitForAnyWorkflowExecutionCompletion,
} from "./event/event-waiter.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "WorkflowOperations" });

/**
 * Fork Configuration
 */
export interface ForkConfig {
  /** Fork operation ID */
  forkId: string;
  /** Fork strategy (serial or parallel) */
  forkStrategy?: "serial" | "parallel";
  /** Starting node ID (optional; the current node of the parent WorkflowExecution is used by default) */
  startNodeId?: string;
  /** Fork path ID (used to identify the path of the child WorkflowExecution) */
  forkPathId?: string;
}

/**
 * Join Strategy
 */
export type JoinStrategy =
  | "ALL_COMPLETED"
  | "ANY_COMPLETED"
  | "ALL_FAILED"
  | "ANY_FAILED"
  | "SUCCESS_COUNT_THRESHOLD";

/**
 * Join result
 */
export interface JoinResult {
  success: boolean;
  output: unknown;
  completedExecutions: WorkflowExecution[];
  failedExecutions: WorkflowExecution[];
}

/**
 * Fork Operation - Creates a child WorkflowExecution
 * @param parentExecutionEntity: Parent WorkflowExecution entity
 * @param forkConfig: Fork configuration
 * @param executionBuilder: WorkflowExecution builder
 * @returns: Child WorkflowExecution entity
 */
export async function fork(
  parentExecutionEntity: WorkflowExecutionEntity,
  forkConfig: ForkConfig,
  executionBuilder: WorkflowExecutionBuilder,
  eventManager?: EventRegistry,
): Promise<WorkflowExecutionEntity> {
  // Step 1: Verify the Fork configuration
  if (!forkConfig.forkId) {
    throw new RuntimeValidationError("Fork config must have forkId", {
      field: "fork.forkId",
    });
  }

  if (
    forkConfig.forkStrategy &&
    forkConfig.forkStrategy !== "serial" &&
    forkConfig.forkStrategy !== "parallel"
  ) {
    throw new RuntimeValidationError(`Invalid forkStrategy: ${forkConfig.forkStrategy}`, {
      field: "fork.forkStrategy",
    });
  }

  // Trigger the WORKFLOW_EXECUTION_FORK_STARTED event
  const forkStartedEvent = buildWorkflowExecutionForkStartedEvent({
    executionId: parentExecutionEntity.id,
    workflowId: parentExecutionEntity.getWorkflowId(),
    parentExecutionId: parentExecutionEntity.id,
    forkConfig: forkConfig as unknown as Record<string, unknown>,
  });
  
  if (eventManager) {
    try {
      await emit(eventManager, forkStartedEvent);
    } catch (error) {
      logger.warn("Failed to emit WORKFLOW_EXECUTION_FORK_STARTED event", { error });
    }
  }

  // Step 2: Create a child WorkflowExecution
  const { workflowExecutionEntity: childExecutionEntity } = await executionBuilder.createFork(parentExecutionEntity, {
    forkId: forkConfig.forkId,
    forkPathId: forkConfig.forkPathId,
    startNodeId: forkConfig.startNodeId,
  });

  // Trigger the WORKFLOW_EXECUTION_FORK_COMPLETED event
  const forkCompletedEvent = buildWorkflowExecutionForkCompletedEvent({
    executionId: parentExecutionEntity.id,
    workflowId: parentExecutionEntity.getWorkflowId(),
    parentExecutionId: parentExecutionEntity.id,
    childExecutionIds: [childExecutionEntity.id],
  });
  
  if (eventManager) {
    try {
      await emit(eventManager, forkCompletedEvent);
    } catch (error) {
      logger.warn("Failed to emit WORKFLOW_EXECUTION_FORK_COMPLETED event", { error });
    }
  }

  return childExecutionEntity;
}

/**
 * Join 操作 - 合并子 WorkflowExecution 结果
 *
 * 说明：
 * - timeout 单位为秒，0 表示不超时
 * - 内部转换为毫秒进行处理
 * - 使用 Promise.race() 实现超时控制
 * - mainPathId 指定主线程路径，会将主线程的对话历史合并到父线程
 *
 * @param childExecutionIds Child execution ID array
 * @param joinStrategy Join strategy
 * @param executionRegistry WorkflowExecution registry
 * @param timeout Timeout (seconds), 0 means no timeout, >0 means timeout in seconds
 * @param parentExecutionId Parent execution ID (optional)
 * @param eventManager Event manager (optional)
 * @param mainPathId Main path ID (optional, defaults to first child execution)
 * @returns Join result
 */
export async function join(
  childExecutionIds: string[],
  joinStrategy: JoinStrategy,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  mainPathId: string,
  timeout: number = 0,
  parentExecutionId?: string,
  eventManager?: EventRegistry,
): Promise<JoinResult> {
  // Step 1: Verify the Join configuration
  if (!joinStrategy) {
    throw new RuntimeValidationError("Join config must have joinStrategy", {
      field: "join.joinStrategy",
    });
  }

  if (timeout < 0) {
    throw new RuntimeValidationError("Join timeout must be non-negative", {
      field: "join.timeout",
    });
  }

  // Trigger the WORKFLOW_EXECUTION_JOIN_STARTED event
  if (eventManager && parentExecutionId) {
    const parentExecutionEntity = workflowExecutionRegistry.get(parentExecutionId);
    if (parentExecutionEntity) {
      const joinStartedEvent = buildWorkflowExecutionJoinStartedEvent({
        executionId: parentExecutionEntity.id,
        workflowId: parentExecutionEntity.getWorkflowId(),
        parentExecutionId: parentExecutionEntity.id,
        childExecutionIds: childExecutionIds,
        joinStrategy,
      });
      
      if (eventManager) {
        try {
          await emit(eventManager, joinStartedEvent);
        } catch (error) {
          logger.warn("Failed to emit WORKFLOW_EXECUTION_JOIN_STARTED event", { error });
        }
      }
    }
  }

  // Step 2: Wait for the child WorkflowExecution to complete
  // If timeout is 0, it means no timeout, pass undefined
  const timeoutMs = timeout > 0 ? timeout * 1000 : undefined;

  // 统一使用 eventManager.waitFor() 的超时机制
  const result = await waitForCompletion(
    childExecutionIds,
    joinStrategy,
    workflowExecutionRegistry,
    timeoutMs,
    parentExecutionId,
    eventManager,
  );

  const completedExecutions = result.completedExecutions;
  const failedExecutions = result.failedExecutions;

  // Step 3: Determine whether to proceed based on the strategy.
  if (!validateJoinStrategy(completedExecutions, failedExecutions, childExecutionIds, joinStrategy)) {
    throw new ExecutionError(
      `Join condition not met: ${joinStrategy}`,
      undefined,
      childExecutionIds[0],
    );
  }

  // Step 4: Merge the results of the sub-executions
  const output = mergeResults(completedExecutions, joinStrategy);

  // Step 5: Merge the main WorkflowExecution's conversation history into the parent WorkflowExecution
  if (parentExecutionId && mainPathId) {
    const parentExecutionEntity = workflowExecutionRegistry.get(parentExecutionId);
    if (parentExecutionEntity) {
      // Find the sub-WorkflowExecution corresponding to the mainPathId.
      const mainExecution = completedExecutions.find(
        WorkflowExecution => WorkflowExecution.forkJoinContext?.forkPathId === mainPathId,
      );

      if (!mainExecution) {
        throw new ExecutionError(
          `Main workflow execution not found for mainPathId: ${mainPathId}`,
          undefined,
          parentExecutionId,
          { mainPathId, completedExecutionIds: completedExecutions.map(t => t.id) },
        );
      }

      const mainExecutionEntity = workflowExecutionRegistry.get(mainExecution.id);
      if (!mainExecutionEntity) {
        throw new ExecutionError(
          `Main WorkflowExecution entity not found for executionId: ${mainExecution.id}`,
          undefined,
          parentExecutionId,
          { mainPathId, mainExecutionId: mainExecution.id },
        );
      }

      try {
        // Use MessageArrayUtils to clone the messages from the main WorkflowExecution and merge them into the parent WorkflowExecution.
        // Use messageHistoryManager directly since WorkflowExecutionEntity no longer holds ConversationManager
        const mainMessages = mainExecutionEntity.messageHistoryManager.getMessages();
        const clonedMessages = MessageArrayUtils.cloneMessages(mainMessages);

        // Add the cloned message to the parent WorkflowExecution.
        for (const msg of clonedMessages) {
          parentExecutionEntity.messageHistoryManager.addMessage(msg);
        }
      } catch (error) {
        throw new ExecutionError(
          `Failed to merge conversation history from main WorkflowExecution`,
          undefined,
          parentExecutionId,
          { mainPathId, mainExecutionId: mainExecution.id, error: getErrorMessage(error) },
          getErrorOrUndefined(error),
        );
      }
    }
  }

  // Step 6: Return the merged result
  return {
    success: true,
    output,
    completedExecutions,
    failedExecutions,
  };
}

/**
 * Copy Operation - Creates a copy of a WorkflowExecution
 * @param sourceExecutionEntity: The source WorkflowExecution entity
 * @param executionBuilder: The WorkflowExecution builder
 * @returns: The copied WorkflowExecution entity
 */
export async function copy(
  sourceExecutionEntity: WorkflowExecutionEntity,
  executionBuilder: WorkflowExecutionBuilder,
  eventManager?: EventRegistry,
): Promise<WorkflowExecutionEntity> {
  // Step 1: Verify that the source WorkflowExecution exists.
  if (!sourceExecutionEntity) {
    throw new ExecutionError(`Source WorkflowExecution entity is null or undefined`, undefined, "");
  }

  // Trigger the WORKFLOW_EXECUTION_COPY_STARTED event
  const copyStartedEvent = buildWorkflowExecutionCopyStartedEvent({
    executionId: sourceExecutionEntity.id,
    workflowId: sourceExecutionEntity.getWorkflowId(),
    sourceExecutionId: sourceExecutionEntity.id,
  });
  
  if (eventManager) {
    try {
      await emit(eventManager, copyStartedEvent);
    } catch (error) {
      logger.warn("Failed to emit WORKFLOW_EXECUTION_COPY_STARTED event", { error });
    }
  }

  // Step 2: Call WorkflowExecutionBuilder to create a new WorkflowExecution
  const { workflowExecutionEntity: copiedExecutionEntity } = await executionBuilder.createCopy(sourceExecutionEntity);

  // Trigger the WORKFLOW_EXECUTION_COPY_COMPLETED event.
  const copyCompletedEvent = buildWorkflowExecutionCopyCompletedEvent({
    executionId: sourceExecutionEntity.id,
    workflowId: sourceExecutionEntity.getWorkflowId(),
    sourceExecutionId: sourceExecutionEntity.id,
    copiedExecutionId: copiedExecutionEntity.id,
  });
  
  if (eventManager) {
    try {
      await emit(eventManager, copyCompletedEvent);
    } catch (error) {
      logger.warn("Failed to emit WORKFLOW_EXECUTION_COPY_COMPLETED event", { error });
    }
  }

  return copiedExecutionEntity;
}

/**
 * Wait for child executions to complete
 * @param childExecutionIds Array of child execution IDs
 * @param joinStrategy Join strategy
 * @param executionRegistry WorkflowExecution registry
 * @param timeout Timeout period in milliseconds
 * @returns Array of completed and failed executions
 */
async function waitForCompletion(
  childExecutionIds: string[],
  joinStrategy: JoinStrategy,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  timeout: number | undefined,
  parentExecutionId?: string,
  eventManager?: EventRegistry,
): Promise<{ completedExecutions: WorkflowExecution[]; failedExecutions: WorkflowExecution[] }> {
  const completedExecutions: WorkflowExecution[] = [];
  const failedExecutions: WorkflowExecution[] = [];

  // If there is no event manager, use a polling approach.
  if (!eventManager) {
    return await waitForCompletionByPolling(
      childExecutionIds,
      joinStrategy,
      workflowExecutionRegistry,
      timeout,
      parentExecutionId,
      eventManager,
    );
  }

  // Waiting using an event-driven approach
  // Choose your waiting method according to your strategy
  let conditionMet: boolean;
  switch (joinStrategy) {
    case "ALL_COMPLETED":
      // Wait for all executions to complete.
      await waitForMultipleWorkflowExecutionsCompleted(eventManager, childExecutionIds, timeout);
      // Collect all completed executions.
      for (const executionId of childExecutionIds) {
        const executionEntity = workflowExecutionRegistry.get(executionId);
        if (executionEntity) {
          const WorkflowExecution = executionEntity.getExecution();
          const status = executionEntity.getStatus();
          if (status === "COMPLETED") {
            completedExecutions.push(WorkflowExecution);
          } else if (status === "FAILED" || status === "CANCELLED") {
            failedExecutions.push(WorkflowExecution);
          }
        }
      }
      conditionMet = failedExecutions.length === 0;
      break;

    case "ANY_COMPLETED": {
      // Wait for any WorkflowExecution to complete.
      await waitForAnyWorkflowExecutionCompleted(eventManager, childExecutionIds, timeout);
      // Collected executions
      for (const executionId of childExecutionIds) {
        const executionEntity = workflowExecutionRegistry.get(executionId);
        if (executionEntity) {
          const WorkflowExecution = executionEntity.getExecution();
          const status = executionEntity.getStatus();
          if (status === "COMPLETED") {
            completedExecutions.push(WorkflowExecution);
          } else if (status === "FAILED" || status === "CANCELLED") {
            failedExecutions.push(WorkflowExecution);
          }
        }
      }
      conditionMet = completedExecutions.length > 0;
      break;
    }

    case "ALL_FAILED":
      // Waiting for all executions to fail.
      await waitForMultipleWorkflowExecutionsCompleted(eventManager, childExecutionIds, timeout);
      // Collect all failed executions.
      for (const executionId of childExecutionIds) {
        const executionEntity = workflowExecutionRegistry.get(executionId);
        if (executionEntity) {
          const WorkflowExecution = executionEntity.getExecution();
          const status = executionEntity.getStatus();
          if (status === "FAILED" || status === "CANCELLED") {
            failedExecutions.push(WorkflowExecution);
          } else if (status === "COMPLETED") {
            completedExecutions.push(WorkflowExecution);
          }
        }
      }
      conditionMet = failedExecutions.length === childExecutionIds.length;
      break;

    case "ANY_FAILED": {
      // Waiting for any WorkflowExecution to fail
      await waitForAnyWorkflowExecutionCompletion(eventManager, childExecutionIds, timeout);
      // Collect all WorkflowExecution states
      for (const executionId of childExecutionIds) {
        const executionEntity = workflowExecutionRegistry.get(executionId);
        if (executionEntity) {
          const WorkflowExecution = executionEntity.getExecution();
          const status = executionEntity.getStatus();
          if (status === "COMPLETED") {
            completedExecutions.push(WorkflowExecution);
          } else if (status === "FAILED" || status === "CANCELLED") {
            failedExecutions.push(WorkflowExecution);
          }
        }
      }
      conditionMet = failedExecutions.length > 0;
      break;
    }

    case "SUCCESS_COUNT_THRESHOLD":
      // Wait for any WorkflowExecution to complete (simplified approach)
      await waitForAnyWorkflowExecutionCompleted(eventManager, childExecutionIds, timeout);
      // Collect all WorkflowExecution states
      for (const executionId of childExecutionIds) {
        const executionEntity = workflowExecutionRegistry.get(executionId);
        if (executionEntity) {
          const WorkflowExecution = executionEntity.getExecution();
          const status = executionEntity.getStatus();
          if (status === "COMPLETED") {
            completedExecutions.push(WorkflowExecution);
          } else if (status === "FAILED" || status === "CANCELLED") {
            failedExecutions.push(WorkflowExecution);
          }
        }
      }
      conditionMet = completedExecutions.length > 0;
      break;

    default:
      throw new RuntimeValidationError(`Invalid join strategy: ${joinStrategy}`, {
        field: "joinStrategy",
        value: joinStrategy,
      });
  }

  // Trigger the WORKFLOW_EXECUTION_JOIN_CONDITION_MET event
  if (parentExecutionId && conditionMet) {
    const parentExecutionEntity = workflowExecutionRegistry.get(parentExecutionId);
    if (parentExecutionEntity) {
      const joinConditionMetEvent = buildWorkflowExecutionJoinConditionMetEvent({
        executionId: parentExecutionEntity.id,
        workflowId: parentExecutionEntity.getWorkflowId(),
        parentExecutionId: parentExecutionEntity.id,
        childExecutionIds: childExecutionIds,
        condition: joinStrategy,
      });
      
      if (eventManager) {
        try {
          await emit(eventManager, joinConditionMetEvent);
        } catch (error) {
          logger.warn("Failed to emit WORKFLOW_EXECUTION_JOIN_CONDITION_MET event", { error });
        }
      }
    }
  }

  return { completedExecutions, failedExecutions };
}

/**
 * Use polling to wait for workflow executions to complete (alternate scenario)
 *
 * Description:
 * - timeout is the timeout value (in milliseconds) passed into Promise.race.
 * - Timeout is no longer checked when polling, it is managed by the outer Promise.race.
 * - The polling period is 100ms
 */
async function waitForCompletionByPolling(
  childExecutionIds: string[],
  joinStrategy: JoinStrategy,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  timeout: number | undefined,
  parentExecutionId?: string,
  eventManager?: EventRegistry,
): Promise<{ completedExecutions: WorkflowExecution[]; failedExecutions: WorkflowExecution[] }> {
  const completedExecutions: WorkflowExecution[] = [];
  const failedExecutions: WorkflowExecution[] = [];
  const pendingExecutions = new Set(childExecutionIds);
  let conditionMet = false;

  // Enter the waiting loop
  while (pendingExecutions.size > 0) {
    // Check the status of the sub-WorkflowExecution.
    for (const executionId of Array.from(pendingExecutions)) {
      const executionEntity = workflowExecutionRegistry.get(executionId);
      if (!executionEntity) {
        continue;
      }

      const WorkflowExecution = executionEntity.getExecution();
      const status = executionEntity.getStatus();
      if (status === "COMPLETED") {
        completedExecutions.push(WorkflowExecution);
        pendingExecutions.delete(executionId);
      } else if (status === "FAILED" || status === "CANCELLED") {
        failedExecutions.push(WorkflowExecution);
        pendingExecutions.delete(executionId);
      }
    }

    // Determine whether to exit based on the strategy.
    if (
      shouldExitWait(
        completedExecutions,
        failedExecutions,
        childExecutionIds,
        joinStrategy,
        pendingExecutions.size,
      )
    ) {
      conditionMet = true;
      break;
    }

    // Wait for a while and then check again.
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  // Trigger the WORKFLOW_EXECUTION_JOIN_CONDITION_MET event
  if (eventManager && parentExecutionId && conditionMet) {
    const parentExecutionEntity = workflowExecutionRegistry.get(parentExecutionId);
    if (parentExecutionEntity) {
      const joinConditionMetEvent = buildWorkflowExecutionJoinConditionMetEvent({
        executionId: parentExecutionEntity.id,
        workflowId: parentExecutionEntity.getWorkflowId(),
        parentExecutionId: parentExecutionEntity.id,
        childExecutionIds: childExecutionIds,
        condition: joinStrategy,
      });
      
      try {
        await emit(eventManager, joinConditionMetEvent);
      } catch (error) {
        logger.warn("Failed to emit WORKFLOW_EXECUTION_JOIN_CONDITION_MET event", { error });
      }
    }
  }

  // Step 7: Return the completed WorkflowExecution array
  return { completedExecutions, failedExecutions };
}

/**
 * Verify whether the Join strategy is satisfied
 * @param completedExecutions: Array of completed executions
 * @param failedExecutions: Array of failed executions
 * @param childExecutionIds: Array of child execution IDs
 * @param joinStrategy: The Join strategy
 * @returns: Whether the strategy is satisfied
 */
function validateJoinStrategy(
  completedExecutions: WorkflowExecution[],
  failedExecutions: WorkflowExecution[],
  childExecutionIds: string[],
  joinStrategy: JoinStrategy,
): boolean {
  switch (joinStrategy) {
    case "ALL_COMPLETED":
      return failedExecutions.length === 0;
    case "ANY_COMPLETED":
      return completedExecutions.length > 0;
    case "ALL_FAILED":
      return failedExecutions.length === childExecutionIds.length;
    case "ANY_FAILED":
      return failedExecutions.length > 0;
    case "SUCCESS_COUNT_THRESHOLD":
      // An additional threshold parameter is required; for now, we are handling it in a simplified manner.
      return completedExecutions.length > 0;
    default:
      return false;
  }
}

/**
 * Determine whether to exit the waiting state
 * @param completedExecutions: Array of completed executions
 * @param failedExecutions: Array of failed executions
 * @param childExecutionIds: Array of child execution IDs
 * @param joinStrategy: Join strategy
 * @param pendingCount: Number of pending executions
 * @returns: Whether it is time to exit
 */
function shouldExitWait(
  completedExecutions: WorkflowExecution[],
  failedExecutions: WorkflowExecution[],
  childExecutionIds: string[],
  joinStrategy: JoinStrategy,
  pendingCount: number,
): boolean {
  switch (joinStrategy) {
    case "ALL_COMPLETED":
      return pendingCount === 0;
    case "ANY_COMPLETED":
      return completedExecutions.length > 0;
    case "ALL_FAILED":
      return pendingCount === 0 && failedExecutions.length === childExecutionIds.length;
    case "ANY_FAILED":
      return failedExecutions.length > 0;
    case "SUCCESS_COUNT_THRESHOLD":
      // An additional threshold parameter is required; for now, we are handling it in a simplified manner.
      return completedExecutions.length > 0;
    default:
      return false;
  }
}

/**
 * Merge the results
 * @param completedExecutions: Array of completed executions
 * @param joinStrategy: Join strategy
 * @returns: Merged output
 */
function mergeResults(completedExecutions: WorkflowExecution[], _joinStrategy: JoinStrategy): unknown {
  if (completedExecutions.length === 0) {
    return {};
  }

  if (completedExecutions.length === 1) {
    return completedExecutions[0]!.output;
  }

  // Merge the outputs of multiple executions
  const mergedOutput: Record<string, unknown> = {};
  for (const WorkflowExecution of completedExecutions) {
    mergedOutput[WorkflowExecution.id] = WorkflowExecution.output;
  }

  return mergedOutput;
}
