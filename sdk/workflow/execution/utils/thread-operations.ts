/**
 * ThreadOperations - Thread Operation Utility Functions
 * Provides stateless thread operations such as Fork/Join/Copy
 * All functions are pure functions and do not hold any state
 */

import type { Thread } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { ThreadBuilder } from "../factories/thread-builder.js";
import type { WorkflowExecutionRegistry } from "../../stores/thread-registry.js";
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
import { safeEmit } from "../../../core/utils/event/event-emitter.js";
import {
  waitForMultipleThreadsCompleted,
  waitForAnyThreadCompleted,
  waitForAnyThreadCompletion,
} from "./event/event-waiter.js";

/**
 * Fork Configuration
 */
export interface ForkConfig {
  /** Fork operation ID */
  forkId: string;
  /** Fork strategy (serial or parallel) */
  forkStrategy?: "serial" | "parallel";
  /** Starting node ID (optional; the current node of the parent thread is used by default) */
  startNodeId?: string;
  /** Fork path ID (used to identify the path of the child thread) */
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
  completedThreads: Thread[];
  failedThreads: Thread[];
}

/**
 * Fork Operation - Creates a child thread
 * @param parentThreadEntity: Parent thread entity
 * @param forkConfig: Fork configuration
 * @param threadBuilder: Thread builder
 * @returns: Child thread entity
 */
export async function fork(
  parentThreadEntity: WorkflowExecutionEntity,
  forkConfig: ForkConfig,
  threadBuilder: ThreadBuilder,
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
    threadId: parentThreadEntity.id,
    workflowId: parentThreadEntity.getWorkflowId(),
    parentExecutionId: parentThreadEntity.id,
    forkConfig: forkConfig as unknown as Record<string, unknown>,
  });
  await safeEmit(eventManager, forkStartedEvent);

  // Step 2: Create a child thread
  const { threadEntity: childThreadEntity } = await threadBuilder.createFork(parentThreadEntity, {
    forkId: forkConfig.forkId,
    forkPathId: forkConfig.forkPathId,
    startNodeId: forkConfig.startNodeId,
  });

  // Trigger the WORKFLOW_EXECUTION_FORK_COMPLETED event
  const forkCompletedEvent = buildWorkflowExecutionForkCompletedEvent({
    threadId: parentThreadEntity.id,
    workflowId: parentThreadEntity.getWorkflowId(),
    parentExecutionId: parentThreadEntity.id,
    childExecutionIds: [childThreadEntity.id],
  });
  await safeEmit(eventManager, forkCompletedEvent);

  return childThreadEntity;
}

/**
 * Join 操作 - 合并子 thread 结果
 *
 * 说明：
 * - timeout 单位为秒，0 表示不超时
 * - 内部转换为毫秒进行处理
 * - 使用 Promise.race() 实现超时控制
 * - mainPathId 指定主线程路径，会将主线程的对话历史合并到父线程
 *
 * @param childThreadIds 子线程 ID 数组
 * @param joinStrategy Join 策略
 * @param threadRegistry Thread 注册表
 * @param timeout 超时时间（秒），0 表示不超时，>0 表示超时的秒数
 * @param parentThreadId 父线程 ID（可选）
 * @param eventManager 事件管理器（可选）
 * @param mainPathId 主线程路径 ID（可选，默认使用第一个子线程）
 * @returns Join 结果
 */
export async function join(
  childThreadIds: string[],
  joinStrategy: JoinStrategy,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  mainPathId: string,
  timeout: number = 0,
  parentThreadId?: string,
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
  if (eventManager && parentThreadId) {
    const parentThreadEntity = threadRegistry.get(parentThreadId);
    if (parentThreadEntity) {
      const joinStartedEvent = buildWorkflowExecutionJoinStartedEvent({
        threadId: parentThreadEntity.id,
        workflowId: parentThreadEntity.getWorkflowId(),
        parentExecutionId: parentThreadEntity.id,
        childExecutionIds: childThreadIds,
        joinStrategy,
      });
      await safeEmit(eventManager, joinStartedEvent);
    }
  }

  // Step 2: Wait for the child thread to complete
  // If timeout is 0, it means no timeout, pass undefined
  const timeoutMs = timeout > 0 ? timeout * 1000 : undefined;

  // 统一使用 eventManager.waitFor() 的超时机制
  const result = await waitForCompletion(
    childThreadIds,
    joinStrategy,
    threadRegistry,
    timeoutMs,
    parentThreadId,
    eventManager,
  );

  const completedThreads = result.completedThreads;
  const failedThreads = result.failedThreads;

  // Step 3: Determine whether to proceed based on the strategy.
  if (!validateJoinStrategy(completedThreads, failedThreads, childThreadIds, joinStrategy)) {
    throw new ExecutionError(
      `Join condition not met: ${joinStrategy}`,
      undefined,
      childThreadIds[0],
    );
  }

  // Step 4: Merge the results of the sub-threads
  const output = mergeResults(completedThreads, joinStrategy);

  // Step 5: Merge the main thread's conversation history into the parent thread
  if (parentThreadId && mainPathId) {
    const parentThreadEntity = threadRegistry.get(parentThreadId);
    if (parentThreadEntity) {
      // Find the sub-thread corresponding to the mainPathId.
      const mainThread = completedThreads.find(
        thread => thread.forkJoinContext?.forkPathId === mainPathId,
      );

      if (!mainThread) {
        throw new ExecutionError(
          `Main thread not found for mainPathId: ${mainPathId}`,
          undefined,
          parentThreadId,
          { mainPathId, completedThreadIds: completedThreads.map(t => t.id) },
        );
      }

      const mainThreadEntity = threadRegistry.get(mainThread.id);
      if (!mainThreadEntity) {
        throw new ExecutionError(
          `Main thread entity not found for threadId: ${mainThread.id}`,
          undefined,
          parentThreadId,
          { mainPathId, mainThreadId: mainThread.id },
        );
      }

      try {
        // Use MessageArrayUtils to clone the messages from the main thread and merge them into the parent thread.
        // Use messageHistoryManager directly since WorkflowExecutionEntity no longer holds ConversationManager
        const mainMessages = mainThreadEntity.messageHistoryManager.getMessages();
        const clonedMessages = MessageArrayUtils.cloneMessages(mainMessages);

        // Add the cloned message to the parent thread.
        for (const msg of clonedMessages) {
          parentThreadEntity.messageHistoryManager.addMessage(msg);
        }
      } catch (error) {
        throw new ExecutionError(
          `Failed to merge conversation history from main thread`,
          undefined,
          parentThreadId,
          { mainPathId, mainThreadId: mainThread.id, error: getErrorMessage(error) },
          getErrorOrUndefined(error),
        );
      }
    }
  }

  // Step 6: Return the merged result
  return {
    success: true,
    output,
    completedThreads,
    failedThreads,
  };
}

/**
 * Copy Operation - Creates a copy of a thread
 * @param sourceThreadEntity: The source thread entity
 * @param threadBuilder: The thread builder
 * @returns: The copied thread entity
 */
export async function copy(
  sourceThreadEntity: WorkflowExecutionEntity,
  threadBuilder: ThreadBuilder,
  eventManager?: EventRegistry,
): Promise<WorkflowExecutionEntity> {
  // Step 1: Verify that the source thread exists.
  if (!sourceThreadEntity) {
    throw new ExecutionError(`Source thread entity is null or undefined`, undefined, "");
  }

  // Trigger the WORKFLOW_EXECUTION_COPY_STARTED event
  const copyStartedEvent = buildWorkflowExecutionCopyStartedEvent({
    threadId: sourceThreadEntity.id,
    workflowId: sourceThreadEntity.getWorkflowId(),
    sourceExecutionId: sourceThreadEntity.id,
  });
  await safeEmit(eventManager, copyStartedEvent);

  // Step 2: Call ThreadBuilder to create a new thread
  const { threadEntity: copiedThreadEntity } = await threadBuilder.createCopy(sourceThreadEntity);

  // Trigger the WORKFLOW_EXECUTION_COPY_COMPLETED event.
  const copyCompletedEvent = buildWorkflowExecutionCopyCompletedEvent({
    threadId: sourceThreadEntity.id,
    workflowId: sourceThreadEntity.getWorkflowId(),
    sourceExecutionId: sourceThreadEntity.id,
    copiedExecutionId: copiedThreadEntity.id,
  });
  await safeEmit(eventManager, copyCompletedEvent);

  return copiedThreadEntity;
}

/**
 * Wait for child threads to complete
 * @param childThreadIds Array of child thread IDs
 * @param joinStrategy Join strategy
 * @param threadRegistry Thread registry
 * @param timeout Timeout period in milliseconds
 * @returns Array of completed and failed threads
 */
async function waitForCompletion(
  childThreadIds: string[],
  joinStrategy: JoinStrategy,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  timeout: number | undefined,
  parentThreadId?: string,
  eventManager?: EventRegistry,
): Promise<{ completedThreads: Thread[]; failedThreads: Thread[] }> {
  const completedThreads: Thread[] = [];
  const failedThreads: Thread[] = [];

  // If there is no event manager, use a polling approach.
  if (!eventManager) {
    return await waitForCompletionByPolling(
      childThreadIds,
      joinStrategy,
      threadRegistry,
      timeout,
      parentThreadId,
      eventManager,
    );
  }

  // Waiting using an event-driven approach
  // Choose your waiting method according to your strategy
  let conditionMet: boolean;
  switch (joinStrategy) {
    case "ALL_COMPLETED":
      // Wait for all threads to complete.
      await waitForMultipleThreadsCompleted(eventManager, childThreadIds, timeout);
      // Collect all completed threads.
      for (const threadId of childThreadIds) {
        const threadEntity = threadRegistry.get(threadId);
        if (threadEntity) {
          const thread = threadEntity.getThread();
          const status = threadEntity.getStatus();
          if (status === "COMPLETED") {
            completedThreads.push(thread);
          } else if (status === "FAILED" || status === "CANCELLED") {
            failedThreads.push(thread);
          }
        }
      }
      conditionMet = failedThreads.length === 0;
      break;

    case "ANY_COMPLETED": {
      // Wait for any thread to complete.
      await waitForAnyThreadCompleted(eventManager, childThreadIds, timeout);
      // Collected threads
      for (const threadId of childThreadIds) {
        const threadEntity = threadRegistry.get(threadId);
        if (threadEntity) {
          const thread = threadEntity.getThread();
          const status = threadEntity.getStatus();
          if (status === "COMPLETED") {
            completedThreads.push(thread);
          } else if (status === "FAILED" || status === "CANCELLED") {
            failedThreads.push(thread);
          }
        }
      }
      conditionMet = completedThreads.length > 0;
      break;
    }

    case "ALL_FAILED":
      // Waiting for all threads to fail.
      await waitForMultipleThreadsCompleted(eventManager, childThreadIds, timeout);
      // Collect all failed threads.
      for (const threadId of childThreadIds) {
        const threadEntity = threadRegistry.get(threadId);
        if (threadEntity) {
          const thread = threadEntity.getThread();
          const status = threadEntity.getStatus();
          if (status === "FAILED" || status === "CANCELLED") {
            failedThreads.push(thread);
          } else if (status === "COMPLETED") {
            completedThreads.push(thread);
          }
        }
      }
      conditionMet = failedThreads.length === childThreadIds.length;
      break;

    case "ANY_FAILED": {
      // Waiting for any thread to fail
      await waitForAnyThreadCompletion(eventManager, childThreadIds, timeout);
      // Collect all thread states
      for (const threadId of childThreadIds) {
        const threadEntity = threadRegistry.get(threadId);
        if (threadEntity) {
          const thread = threadEntity.getThread();
          const status = threadEntity.getStatus();
          if (status === "COMPLETED") {
            completedThreads.push(thread);
          } else if (status === "FAILED" || status === "CANCELLED") {
            failedThreads.push(thread);
          }
        }
      }
      conditionMet = failedThreads.length > 0;
      break;
    }

    case "SUCCESS_COUNT_THRESHOLD":
      // Wait for any thread to complete (simplified approach)
      await waitForAnyThreadCompleted(eventManager, childThreadIds, timeout);
      // Collect all thread states
      for (const threadId of childThreadIds) {
        const threadEntity = threadRegistry.get(threadId);
        if (threadEntity) {
          const thread = threadEntity.getThread();
          const status = threadEntity.getStatus();
          if (status === "COMPLETED") {
            completedThreads.push(thread);
          } else if (status === "FAILED" || status === "CANCELLED") {
            failedThreads.push(thread);
          }
        }
      }
      conditionMet = completedThreads.length > 0;
      break;

    default:
      throw new RuntimeValidationError(`Invalid join strategy: ${joinStrategy}`, {
        field: "joinStrategy",
        value: joinStrategy,
      });
  }

  // Trigger the WORKFLOW_EXECUTION_JOIN_CONDITION_MET event
  if (parentThreadId && conditionMet) {
    const parentThreadEntity = threadRegistry.get(parentThreadId);
    if (parentThreadEntity) {
      const joinConditionMetEvent = buildWorkflowExecutionJoinConditionMetEvent({
        threadId: parentThreadEntity.id,
        workflowId: parentThreadEntity.getWorkflowId(),
        parentExecutionId: parentThreadEntity.id,
        childExecutionIds: childThreadIds,
        condition: joinStrategy,
      });
      await safeEmit(eventManager, joinConditionMetEvent);
    }
  }

  return { completedThreads, failedThreads };
}

/**
 * Use polling to wait for threads to complete (alternate scenario)
 *
 * Description:
 * - timeout is the timeout value (in milliseconds) passed into Promise.race.
 * - Timeout is no longer checked when polling, it is managed by the outer Promise.race.
 * - The polling period is 100ms
 */
async function waitForCompletionByPolling(
  childThreadIds: string[],
  joinStrategy: JoinStrategy,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  timeout: number | undefined,
  parentThreadId?: string,
  eventManager?: EventRegistry,
): Promise<{ completedThreads: Thread[]; failedThreads: Thread[] }> {
  const completedThreads: Thread[] = [];
  const failedThreads: Thread[] = [];
  const pendingThreads = new Set(childThreadIds);
  let conditionMet = false;

  // Enter the waiting loop
  while (pendingThreads.size > 0) {
    // Check the status of the sub-thread.
    for (const threadId of Array.from(pendingThreads)) {
      const threadEntity = threadRegistry.get(threadId);
      if (!threadEntity) {
        continue;
      }

      const thread = threadEntity.getThread();
      const status = threadEntity.getStatus();
      if (status === "COMPLETED") {
        completedThreads.push(thread);
        pendingThreads.delete(threadId);
      } else if (status === "FAILED" || status === "CANCELLED") {
        failedThreads.push(thread);
        pendingThreads.delete(threadId);
      }
    }

    // Determine whether to exit based on the strategy.
    if (
      shouldExitWait(
        completedThreads,
        failedThreads,
        childThreadIds,
        joinStrategy,
        pendingThreads.size,
      )
    ) {
      conditionMet = true;
      break;
    }

    // Wait for a while and then check again.
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  // Trigger the WORKFLOW_EXECUTION_JOIN_CONDITION_MET event
  if (eventManager && parentThreadId && conditionMet) {
    const parentThreadEntity = threadRegistry.get(parentThreadId);
    if (parentThreadEntity) {
      const joinConditionMetEvent = buildWorkflowExecutionJoinConditionMetEvent({
        threadId: parentThreadEntity.id,
        workflowId: parentThreadEntity.getWorkflowId(),
        parentExecutionId: parentThreadEntity.id,
        childExecutionIds: childThreadIds,
        condition: joinStrategy,
      });
      await safeEmit(eventManager, joinConditionMetEvent);
    }
  }

  // Step 7: Return the completed thread array
  return { completedThreads, failedThreads };
}

/**
 * Verify whether the Join strategy is satisfied
 * @param completedThreads: Array of completed threads
 * @param failedThreads: Array of failed threads
 * @param childThreadIds: Array of child thread IDs
 * @param joinStrategy: The Join strategy
 * @returns: Whether the strategy is satisfied
 */
function validateJoinStrategy(
  completedThreads: Thread[],
  failedThreads: Thread[],
  childThreadIds: string[],
  joinStrategy: JoinStrategy,
): boolean {
  switch (joinStrategy) {
    case "ALL_COMPLETED":
      return failedThreads.length === 0;
    case "ANY_COMPLETED":
      return completedThreads.length > 0;
    case "ALL_FAILED":
      return failedThreads.length === childThreadIds.length;
    case "ANY_FAILED":
      return failedThreads.length > 0;
    case "SUCCESS_COUNT_THRESHOLD":
      // An additional threshold parameter is required; for now, we are handling it in a simplified manner.
      return completedThreads.length > 0;
    default:
      return false;
  }
}

/**
 * Determine whether to exit the waiting state
 * @param completedThreads: Array of completed threads
 * @param failedThreads: Array of failed threads
 * @param childThreadIds: Array of child thread IDs
 * @param joinStrategy: Join strategy
 * @param pendingCount: Number of pending threads
 * @returns: Whether it is time to exit
 */
function shouldExitWait(
  completedThreads: Thread[],
  failedThreads: Thread[],
  childThreadIds: string[],
  joinStrategy: JoinStrategy,
  pendingCount: number,
): boolean {
  switch (joinStrategy) {
    case "ALL_COMPLETED":
      return pendingCount === 0;
    case "ANY_COMPLETED":
      return completedThreads.length > 0;
    case "ALL_FAILED":
      return pendingCount === 0 && failedThreads.length === childThreadIds.length;
    case "ANY_FAILED":
      return failedThreads.length > 0;
    case "SUCCESS_COUNT_THRESHOLD":
      // An additional threshold parameter is required; for now, we are handling it in a simplified manner.
      return completedThreads.length > 0;
    default:
      return false;
  }
}

/**
 * Merge the results
 * @param completedThreads: Array of completed threads
 * @param joinStrategy: Join strategy
 * @returns: Merged output
 */
function mergeResults(completedThreads: Thread[], _joinStrategy: JoinStrategy): unknown {
  if (completedThreads.length === 0) {
    return {};
  }

  if (completedThreads.length === 1) {
    return completedThreads[0]!.output;
  }

  // Merge the outputs of multiple threads
  const mergedOutput: Record<string, unknown> = {};
  for (const thread of completedThreads) {
    mergedOutput[thread.id] = thread.output;
  }

  return mergedOutput;
}
