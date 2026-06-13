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
import type { WorkflowStateCoordinator } from "../../state-managers/workflow-state-coordinator.js";
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
} from "../../../core/utils/event/builders/index.js";
import { emit } from "../../../core/utils/event/emit-event.js";
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

  // Step 2: Create a child WorkflowExecution using unified API
  const { workflowExecutionEntity: childExecutionEntity } =
    await executionBuilder.createChildExecution(parentExecutionEntity, {
      type: "FORK_BRANCH",
      config: {
        forkPathId: forkConfig.forkPathId,
        startNodeId: forkConfig.startNodeId,
      },
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
 * - mainPathId 指定主线程路径（向后兼容）
 * - variableOutputs 支持从分支显式导出变量到父工作流
 * - messageOutputs 使用 boundary-config 模式显式定义消息上下文输出
 *
 * @param childExecutionIds Child execution ID array
 * @param joinStrategy Join strategy
 * @param workflowExecutionRegistry WorkflowExecution registry
 * @param mainPathId Main path ID (optional, for backward compatibility)
 * @param timeout Timeout (seconds), 0 means no timeout, >0 means timeout in seconds
 * @param parentExecutionId Parent execution ID (optional)
 * @param eventManager Event manager (optional)
 * @param variableOutputs Variable output mappings for explicit export from branches (optional)
 * @param messageOutputs Message context output mappings using boundary-config pattern (optional)
 * @param stateCoordinatorMap Map of execution IDs to state coordinators (optional, for new architecture)
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
  variableOutputs?: Array<{ sourcePathId: string; variableName: string; targetName?: string }>,
  messageOutputs?: Array<{ sourcePathId: string; internalName: string; externalName: string }>,
  stateCoordinatorMap?: Map<string, WorkflowStateCoordinator>,
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
  if (
    !validateJoinStrategy(completedExecutions, failedExecutions, childExecutionIds, joinStrategy)
  ) {
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
        // Export message contexts from branches to parent workflow if messageOutputs is configured
        // This uses the same boundary-config pattern as START/END nodes for consistency
        if (messageOutputs && messageOutputs.length > 0) {
          logger.debug("Exporting message contexts from branches to parent workflow", {
            messageOutputCount: messageOutputs.length,
            parentExecutionId,
          });

          for (const msgOutput of messageOutputs) {
            // Find the source execution by forkPathId
            const sourceExecution = completedExecutions.find(
              exec => exec.forkJoinContext?.forkPathId === msgOutput.sourcePathId,
            );

            if (!sourceExecution) {
              logger.warn(`Source execution not found for pathId: ${msgOutput.sourcePathId}`, {
                availablePaths: completedExecutions.map(e => e.forkJoinContext?.forkPathId),
              });
              continue;
            }

            const sourceEntity = workflowExecutionRegistry.get(sourceExecution.id);
            if (!sourceEntity) {
              logger.warn(`Source execution entity not found: ${sourceExecution.id}`);
              continue;
            }

            // Get messages from the source branch's main context
            const sourceStateCoordinator = stateCoordinatorMap?.get(sourceExecution.id);
            if (!sourceStateCoordinator) {
              logger.warn(
                `State coordinator not found for source execution: ${sourceExecution.id}`,
              );
              continue;
            }

            const sourceMessages = sourceStateCoordinator.exportMessagesForChild();

            if (!sourceMessages || sourceMessages.length === 0) {
              logger.debug(`No messages found in source execution`, {
                sourcePathId: msgOutput.sourcePathId,
                sourceExecutionId: sourceExecution.id,
              });
              continue;
            }

            // Clone messages to avoid reference sharing
            const clonedMessages = MessageArrayUtils.cloneMessages(sourceMessages);

            // Add messages to parent execution
            const parentStateCoordinator = stateCoordinatorMap?.get(parentExecutionId);
            if (!parentStateCoordinator) {
              logger.warn(`State coordinator not found for parent execution: ${parentExecutionId}`);
              continue;
            }

            parentStateCoordinator.importMessagesFromChild(clonedMessages);

            logger.debug("Message context exported from branch to parent", {
              sourcePathId: msgOutput.sourcePathId,
              internalName: msgOutput.internalName,
              externalName: msgOutput.externalName,
              messageCount: clonedMessages.length,
              parentExecutionId,
            });
          }
        } else if (mainPathId) {
          // Backward compatibility: if no messageOutputs configured, use old behavior
          // Merge messages from main path only
          const mainExecution = completedExecutions.find(
            WorkflowExecution => WorkflowExecution.forkJoinContext?.forkPathId === mainPathId,
          );

          if (mainExecution) {
            const mainStateCoordinator = stateCoordinatorMap?.get(mainExecution.id);
            if (!mainStateCoordinator) {
              logger.warn(`State coordinator not found for main execution: ${mainExecution.id}`);
            } else {
              // Get messages from main path
              const mainMessages = mainStateCoordinator.exportMessagesForChild();
              const clonedMessages = MessageArrayUtils.cloneMessages(mainMessages);

              // Add messages to parent
              const parentStateCoordinator = stateCoordinatorMap?.get(parentExecutionId);
              if (!parentStateCoordinator) {
                logger.warn(
                  `State coordinator not found for parent execution: ${parentExecutionId}`,
                );
              } else {
                parentStateCoordinator.importMessagesFromChild(clonedMessages);

                logger.debug("Backward compatible message merge from main path", {
                  mainPathId,
                  messageCount: clonedMessages.length,
                });
              }
            }
          }
        }
      } catch (error) {
        throw new ExecutionError(
          `Failed to export message contexts from branches`,
          undefined,
          parentExecutionId,
          { error: getErrorMessage(error) },
          getErrorOrUndefined(error),
        );
      }

      // Export variables from branches to parent workflow if variableOutputs is configured
      if (variableOutputs && variableOutputs.length > 0) {
        try {
          logger.debug("Exporting variables from branches to parent workflow", {
            variableOutputCount: variableOutputs.length,
            parentExecutionId,
          });

          for (const varOutput of variableOutputs) {
            // Find the source execution by forkPathId
            const sourceExecution = completedExecutions.find(
              exec => exec.forkJoinContext?.forkPathId === varOutput.sourcePathId,
            );

            if (!sourceExecution) {
              logger.warn(`Source execution not found for pathId: ${varOutput.sourcePathId}`, {
                availablePaths: completedExecutions.map(e => e.forkJoinContext?.forkPathId),
              });
              continue;
            }

            const sourceEntity = workflowExecutionRegistry.get(sourceExecution.id);
            if (!sourceEntity) {
              logger.warn(`Source execution entity not found: ${sourceExecution.id}`);
              continue;
            }

            // Get the variable value from source
            const variableValue = sourceEntity.variableStateManager.getVariable(
              varOutput.variableName,
            );

            if (variableValue === undefined) {
              logger.warn(`Variable not found in source execution`, {
                variableName: varOutput.variableName,
                sourcePathId: varOutput.sourcePathId,
                sourceExecutionId: sourceExecution.id,
              });
              continue;
            }

            // Set the variable in parent execution (with optional renaming)
            const targetName = varOutput.targetName || varOutput.variableName;
            parentExecutionEntity.variableStateManager.setVariable(targetName, variableValue);

            logger.debug("Variable exported from branch to parent", {
              sourcePathId: varOutput.sourcePathId,
              variableName: varOutput.variableName,
              targetName,
              parentExecutionId,
            });
          }
        } catch (error) {
          logger.error("Failed to export variables from branches", {
            error: getErrorMessage(error),
            parentExecutionId,
          });
          // Don't throw here - variable export failure shouldn't fail the entire join
        }
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
 * @param sourceStateCoordinator: Source execution's state coordinator (required for message export)
 * @returns: The copied WorkflowExecution entity
 */
export async function copy(
  sourceExecutionEntity: WorkflowExecutionEntity,
  executionBuilder: WorkflowExecutionBuilder,
  sourceStateCoordinator: WorkflowStateCoordinator,
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
  const { workflowExecutionEntity: copiedExecutionEntity } = await executionBuilder.createCopy(
    sourceExecutionEntity,
    sourceStateCoordinator,
  );

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
 * Wait for child executions to complete using event-driven approach
 * @param childExecutionIds Array of child execution IDs
 * @param joinStrategy Join strategy
 * @param executionRegistry WorkflowExecution registry
 * @param timeout Timeout period in milliseconds
 * @param parentExecutionId Parent execution ID for event emission
 * @param eventManager Event registry (required)
 * @returns Array of completed and failed executions
 * @throws Error if eventManager is not provided
 */
async function waitForCompletion(
  childExecutionIds: string[],
  joinStrategy: JoinStrategy,
  workflowExecutionRegistry: WorkflowExecutionRegistry,
  timeout: number | undefined,
  parentExecutionId?: string,
  eventManager?: EventRegistry,
): Promise<{ completedExecutions: WorkflowExecution[]; failedExecutions: WorkflowExecution[] }> {
  // EventManager is required - no fallback to polling
  if (!eventManager) {
    throw new ExecutionError(
      "EventManager is required for waiting on child executions. Polling fallback has been removed.",
      undefined,
      "MISSING_EVENT_MANAGER",
    );
  }

  const completedExecutions: WorkflowExecution[] = [];
  const failedExecutions: WorkflowExecution[] = [];

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
          const WorkflowExecution = executionEntity.getWorkflowExecutionData();
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
          const WorkflowExecution = executionEntity.getWorkflowExecutionData();
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
          const WorkflowExecution = executionEntity.getWorkflowExecutionData();
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
          const WorkflowExecution = executionEntity.getWorkflowExecutionData();
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
          const WorkflowExecution = executionEntity.getWorkflowExecutionData();
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
 * Merge the results
 * @param completedExecutions: Array of completed executions
 * @param joinStrategy: Join strategy
 * @returns: Merged output
 */
function mergeResults(
  completedExecutions: WorkflowExecution[],
  _joinStrategy: JoinStrategy,
): unknown {
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
