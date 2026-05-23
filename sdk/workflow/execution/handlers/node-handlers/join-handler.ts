/**
 * Join Node Processing Function
 *
 * The Join node serves as the synchronization point for FORK branches.
 * It implements the actual aggregation logic for:
 * - variableOutputs: collecting variables from completed branches
 * - messageOutputs: collecting message contexts from completed branches
 * - dataOutputs: collecting data outputs from completed branches
 * - join strategy validation (ALL_COMPLETED, ANY_COMPLETED, etc.)
 *
 * The handler uses the parent execution's SyncBarrier to locate completed
 * branch execution entities and aggregates their results back to the parent.
 */

import type {
  RuntimeNode,
  JoinNodeConfig,
  JoinNodeOutput,
  MessageContextRegistry,
  WorkflowExecution,
} from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { GlobalContext } from "../../../../core/global-context.js";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import { RuntimeValidationError } from "@wf-agent/types";
import { now, getErrorOrNew } from "@wf-agent/common-utils";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { emit } from "../../../../core/utils/event/event-emitter.js";
import {
  buildWorkflowExecutionJoinStartedEvent,
  buildWorkflowExecutionJoinConditionMetEvent,
  buildWorkflowExecutionJoinCompletedEvent,
  buildWorkflowExecutionJoinFailedEvent,
} from "../../utils/event/index.js";

const logger = createContextualLogger({ component: "join-handler" });

/**
 * Check if the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity): boolean {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return false;
  }
  return true;
}

/**
 * Collect branch execution entities by fork path IDs using parent's SyncBarrier.
 *
 * Uses the parent execution's SyncBarrier to map forkPathId → executionId,
 * then retrieves the WorkflowExecutionEntity from the registry.
 *
 * @param parentEntity The parent workflow execution entity (has SyncBarrier)
 * @param forkPathIds Array of fork path IDs to look up
 * @param executionRegistry Registry to retrieve execution entities
 * @returns Map of forkPathId → WorkflowExecutionEntity for completed branches
 */
function collectBranches(
  parentEntity: WorkflowExecutionEntity,
  forkPathIds: string[],
  executionRegistry: WorkflowExecutionRegistry,
): {
  completedBranches: Map<string, WorkflowExecutionEntity>;
  failedBranches: string[];
  skippedBranches: string[];
} {
  const syncBarrier = parentEntity.getSyncBarrier();
  if (!syncBarrier) {
    logger.warn(
      "Parent execution has no SyncBarrier - FORK node may not have been processed correctly",
      {
        parentExecutionId: parentEntity.id,
      },
    );
    return { completedBranches: new Map(), failedBranches: [], skippedBranches: [] };
  }

  const completedBranches = new Map<string, WorkflowExecutionEntity>();
  const failedBranches: string[] = [];
  const skippedBranches: string[] = [];

  for (const pathId of forkPathIds) {
    const executionId = syncBarrier.getExecutionIdByPath(pathId);
    if (!executionId) {
      logger.warn("Fork path not registered in SyncBarrier", {
        forkPathId: pathId,
        availablePaths: syncBarrier.getAllPathIds(),
      });
      skippedBranches.push(pathId);
      continue;
    }

    const branchEntity = executionRegistry.get(executionId);
    if (!branchEntity) {
      logger.warn("Branch execution entity not found in registry", {
        forkPathId: pathId,
        executionId,
      });
      skippedBranches.push(pathId);
      continue;
    }

    const status = branchEntity.getStatus();
    if (status === "COMPLETED") {
      completedBranches.set(pathId, branchEntity);
    } else if (status === "FAILED" || status === "CANCELLED") {
      failedBranches.push(pathId);
    } else {
      skippedBranches.push(pathId);
    }
  }

  return { completedBranches, failedBranches, skippedBranches };
}

/**
 * Evaluate join strategy to determine if execution should proceed.
 *
 * @param strategy The join strategy
 * @param completedCount Number of completed branches
 * @param failedCount Number of failed branches
 * @param totalCount Total number of branches
 * @param threshold Threshold for SUCCESS_COUNT_THRESHOLD strategy
 * @returns Object with shouldProceed flag and error message if applicable
 */
function evaluateStrategy(
  strategy: JoinNodeConfig["joinStrategy"],
  completedCount: number,
  failedCount: number,
  totalCount: number,
  threshold?: number,
): { shouldProceed: boolean; error?: string } {
  switch (strategy) {
    case "ALL_COMPLETED": {
      const allCompleted = completedCount === totalCount;
      return {
        shouldProceed: allCompleted,
        error: allCompleted
          ? undefined
          : `JOIN strategy ALL_COMPLETED not met: ${completedCount}/${totalCount} completed`,
      };
    }

    case "ANY_COMPLETED": {
      const anyCompleted = completedCount > 0;
      return {
        shouldProceed: anyCompleted,
        error: anyCompleted
          ? undefined
          : "JOIN strategy ANY_COMPLETED not met: no completed branches",
      };
    }

    case "ALL_FAILED": {
      const allFailed = failedCount === totalCount;
      return {
        shouldProceed: allFailed,
        error: allFailed
          ? undefined
          : `JOIN strategy ALL_FAILED not met: ${failedCount}/${totalCount} failed`,
      };
    }

    case "ANY_FAILED": {
      const anyFailed = failedCount > 0;
      return {
        shouldProceed: anyFailed,
        error: anyFailed ? undefined : "JOIN strategy ANY_FAILED not met: no failed branches",
      };
    }

    case "SUCCESS_COUNT_THRESHOLD": {
      const metThreshold = completedCount >= (threshold ?? 1);
      return {
        shouldProceed: metThreshold,
        error: metThreshold
          ? undefined
          : `JOIN strategy SUCCESS_COUNT_THRESHOLD not met: ${completedCount}/${threshold ?? 1} required`,
      };
    }

    default:
      return {
        shouldProceed: true,
        error: undefined,
      };
  }
}

/**
 * Get the MessageContextRegistry from a WorkflowExecutionEntity.
 * The registry is attached to the WorkflowExecution data object.
 */
function getMessageContextRegistry(
  entity: WorkflowExecutionEntity,
): MessageContextRegistry | undefined {
  const execution = entity.getExecution();
  return (execution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry })
    .messageContextRegistry;
}

/**
 * Aggregate variable outputs from completed branches into the parent VariableManager.
 *
 * For variableOutputs, we collect from the main branch (mainPathId).
 * If main branch is not completed, fall back to the first completed branch.
 *
 * @param parentEntity The parent workflow execution entity
 * @param completedBranches Map of completed branches (forkPathId → entity)
 * @param mainPathId The main execution path ID
 * @param variableOutputs Array of variable output mappings
 */
function aggregateVariableOutputs(
  parentEntity: WorkflowExecutionEntity,
  completedBranches: Map<string, WorkflowExecutionEntity>,
  mainPathId: string,
  variableOutputs: JoinNodeConfig["variableOutputs"],
): void {
  if (!variableOutputs || variableOutputs.length === 0) {
    return;
  }

  // Determine source branch: prefer mainPathId, fall back to first completed
  let sourceBranch = completedBranches.get(mainPathId);
  if (!sourceBranch) {
    sourceBranch = completedBranches.values().next().value;
    if (!sourceBranch) {
      logger.warn("No completed branches available for variable output aggregation");
      return;
    }
    logger.debug("Main branch not completed, using first completed branch for variable outputs", {
      mainPathId,
      usingPathId: Array.from(completedBranches.keys())[0],
    });
  }

  // Export variables from source branch to parent using the output mappings
  // variableOutputs: [{ internalName: branchVar, externalName: parentVar }]
  // This is semantically an export from branch → parent
  sourceBranch.variableStateManager.exportVariables(
    parentEntity.variableStateManager,
    variableOutputs,
  );

  logger.debug("Aggregated variable outputs", {
    mainPathId,
    variableCount: variableOutputs.length,
    sourceExecutionId: sourceBranch.id,
  });
}

/**
 * Aggregate message context outputs from completed branches into the parent registry.
 *
 * Each messageOutput entry has a sourcePathId that specifies which branch to read from.
 *
 * @param parentEntity The parent workflow execution entity
 * @param completedBranches Map of completed branches (forkPathId → entity)
 * @param messageOutputs Array of message output mappings with sourcePathId
 */
function aggregateMessageOutputs(
  parentEntity: WorkflowExecutionEntity,
  completedBranches: Map<string, WorkflowExecutionEntity>,
  messageOutputs: JoinNodeConfig["messageOutputs"],
): void {
  if (!messageOutputs || messageOutputs.length === 0) {
    return;
  }

  const parentRegistry = getMessageContextRegistry(parentEntity);
  if (!parentRegistry) {
    logger.warn("Parent MessageContextRegistry not available for message output aggregation");
    return;
  }

  for (const outputDef of messageOutputs) {
    const { internalName, externalName, sourcePathId } = outputDef;
    const sourceBranch = completedBranches.get(sourcePathId);

    if (!sourceBranch) {
      logger.warn("Source branch not found or not completed for message output", {
        sourcePathId,
        internalName,
        externalName,
      });
      continue;
    }

    const branchRegistry = getMessageContextRegistry(sourceBranch);
    if (!branchRegistry) {
      logger.warn("Branch MessageContextRegistry not available", {
        sourcePathId,
        branchExecutionId: sourceBranch.id,
      });
      continue;
    }

    const sourceContext = branchRegistry.get(internalName);
    if (!sourceContext) {
      logger.debug("Message context not found in source branch", {
        sourcePathId,
        contextId: internalName,
      });
      continue;
    }

    parentRegistry.register({
      id: externalName,
      messages: [...sourceContext.messages],
      createdAt: now(),
      updatedAt: now(),
      metadata: {
        ...sourceContext.metadata,
        sourcePathId,
        sourceBranchExecutionId: sourceBranch.id,
        joinedAt: Date.now(),
      } as Record<string, unknown>,
    });

    logger.debug("Aggregated message context output", {
      sourcePathId,
      internalName,
      externalName,
      messageCount: sourceContext.messages.length,
    });
  }
}

/**
 * Aggregate data outputs from completed branches into the parent execution output.
 *
 * For dataOutputs, we collect from the main branch's VariableManager
 * and set values into the parent's execution output.
 *
 * @param parentEntity The parent workflow execution entity
 * @param completedBranches Map of completed branches (forkPathId → entity)
 * @param mainPathId The main execution path ID
 * @param dataOutputs Array of data output mappings
 */
function aggregateDataOutputs(
  parentEntity: WorkflowExecutionEntity,
  completedBranches: Map<string, WorkflowExecutionEntity>,
  mainPathId: string,
  dataOutputs: JoinNodeConfig["dataOutputs"],
): void {
  if (!dataOutputs || dataOutputs.length === 0) {
    return;
  }

  let sourceBranch = completedBranches.get(mainPathId);
  if (!sourceBranch) {
    sourceBranch = completedBranches.values().next().value;
    if (!sourceBranch) {
      logger.warn("No completed branches available for data output aggregation");
      return;
    }
  }

  const currentOutput = parentEntity.getOutput() || {};
  const updatedOutput = { ...currentOutput };

  for (const outputDef of dataOutputs) {
    const { internalName, outputKey } = outputDef;
    const value = sourceBranch.variableStateManager.getVariable(internalName);

    if (value !== undefined) {
      updatedOutput[outputKey] = value;
      logger.debug("Aggregated data output", {
        sourceBranchId: sourceBranch.id,
        internalName,
        outputKey,
      });
    }
  }

  parentEntity.setOutput(updatedOutput);

  logger.debug("Aggregated data outputs", {
    mainPathId,
    dataOutputCount: dataOutputs.length,
  });
}

/**
 * Join Node Processing Function
 *
 * Aggregates results from completed FORK branches:
 * 1. Evaluates join strategy to determine if execution should proceed
 * 2. Collects variable outputs from completed branches
 * 3. Collects message context outputs from completed branches
 * 4. Collects data outputs from completed branches
 *
 * Note: This handler returns only JoinNodeOutput-specific fields.
 * The base fields (nodeId, nodeType, status, step, executionTime) are
 * added by the calling executor/workflow coordinator.
 *
 * @param globalContext Global application context (for registry access)
 * @param workflowExecutionEntity Parent workflow execution entity
 * @param node Runtime node definition
 * @returns Join result with aggregated output
 */
export async function joinHandler(
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
): Promise<JoinNodeOutput> {
  if (!canExecute(workflowExecutionEntity)) {
    return {
      completedBranches: [],
      failedBranches: [],
      skippedBranches: [],
      strategy: "SKIPPED",
    };
  }

  const config = node.config as JoinNodeConfig;
  const startTime = Date.now();

  // Get required dependencies from global context
  const executionRegistry = globalContext.container.get(
    Identifiers.WorkflowExecutionRegistry,
  ) as WorkflowExecutionRegistry;

  const eventManager = globalContext.container.get(Identifiers.EventRegistry) as EventRegistry;

  if (!executionRegistry) {
    throw new RuntimeValidationError("WorkflowExecutionRegistry not available in global context", {
      operation: "joinHandler",
      field: "executionRegistry",
    });
  }

  if (!eventManager) {
    throw new RuntimeValidationError("EventRegistry not available in global context", {
      operation: "joinHandler",
      field: "eventRegistry",
    });
  }

  const completedPathIds: string[] = [];

  try {
    logger.info("Executing JOIN node", {
      nodeId: node.id,
      strategy: config.joinStrategy,
      forkPathIds: config.forkPathIds,
      mainPathId: config.mainPathId,
    });

    // Step 1: Collect completed branches via SyncBarrier
    const { completedBranches, failedBranches, skippedBranches } = collectBranches(
      workflowExecutionEntity,
      config.forkPathIds,
      executionRegistry,
    );

    completedPathIds.push(...Array.from(completedBranches.keys()));

    // Emit JOIN_STARTED event with populated childExecutionIds
    await emit(
      eventManager,
      buildWorkflowExecutionJoinStartedEvent({
        parentExecutionId: workflowExecutionEntity.id,
        childExecutionIds: completedPathIds,
        joinStrategy: config.joinStrategy,
      }),
    );

    const totalBranches = config.forkPathIds.length;

    logger.debug("Branch status summary", {
      totalBranches,
      completedCount: completedBranches.size,
      failedCount: failedBranches.length,
      skippedCount: skippedBranches.length,
      completedPaths: completedPathIds,
      failedPaths: failedBranches,
    });

    // Emit JOIN_CONDITION_EVALUATED event
    await emit(
      eventManager,
      buildWorkflowExecutionJoinConditionMetEvent({
        parentExecutionId: workflowExecutionEntity.id,
        childExecutionIds: completedPathIds,
        condition: `join:${config.joinStrategy}:completed=${completedBranches.size}:failed=${failedBranches.length}`,
      }),
    );

    // Step 2: Evaluate join strategy
    const strategyResult = evaluateStrategy(
      config.joinStrategy,
      completedBranches.size,
      failedBranches.length,
      totalBranches,
      config.threshold,
    );

    if (!strategyResult.shouldProceed) {
      const executionDuration = Date.now() - startTime;
      logger.warn("JOIN strategy condition not met", {
        strategy: config.joinStrategy,
        error: strategyResult.error,
      });

      // Emit JOIN_FAILED event
      await emit(
        eventManager,
        buildWorkflowExecutionJoinFailedEvent({
          parentExecutionId: workflowExecutionEntity.id,
          childExecutionIds: completedPathIds,
          joinStrategy: config.joinStrategy,
          error: new Error(strategyResult.error || "Strategy condition not met"),
        }),
      );

      return {
        completedBranches: completedPathIds,
        failedBranches,
        skippedBranches,
        strategy: config.joinStrategy,
        aggregatedOutput: { error: strategyResult.error, executionTime: executionDuration },
      };
    }

    // Step 3: Aggregate variable outputs from main branch
    aggregateVariableOutputs(
      workflowExecutionEntity,
      completedBranches,
      config.mainPathId,
      config.variableOutputs,
    );

    // Step 4: Aggregate message context outputs
    aggregateMessageOutputs(workflowExecutionEntity, completedBranches, config.messageOutputs);

    // Step 5: Aggregate data outputs from main branch
    aggregateDataOutputs(
      workflowExecutionEntity,
      completedBranches,
      config.mainPathId,
      config.dataOutputs,
    );

    const executionDuration = Date.now() - startTime;
    logger.info("JOIN node completed", {
      nodeId: node.id,
      completedBranches: completedPathIds,
      variableOutputCount: config.variableOutputs?.length || 0,
      messageOutputCount: config.messageOutputs?.length || 0,
      dataOutputCount: config.dataOutputs?.length || 0,
      duration: executionDuration,
    });

    // Emit JOIN_COMPLETED event
    const aggregatedOutputCount =
      (config.variableOutputs?.length || 0) +
      (config.messageOutputs?.length || 0) +
      (config.dataOutputs?.length || 0);
    await emit(
      eventManager,
      buildWorkflowExecutionJoinCompletedEvent({
        parentExecutionId: workflowExecutionEntity.id,
        childExecutionIds: completedPathIds,
        joinStrategy: config.joinStrategy,
        aggregatedOutputCount,
        duration: executionDuration,
      }),
    );

    return {
      completedBranches: completedPathIds,
      failedBranches: [],
      skippedBranches: [],
      strategy: config.joinStrategy,
      aggregatedOutput:
        completedPathIds.length > 0
          ? {
              variableCount: config.variableOutputs?.length || 0,
              messageCount: config.messageOutputs?.length || 0,
              dataOutputCount: config.dataOutputs?.length || 0,
            }
          : undefined,
    };
  } catch (error) {
    const executionDuration = Date.now() - startTime;
    logger.error("JOIN node failed with unexpected error", {
      nodeId: node.id,
      error,
    });

    // Emit JOIN_FAILED event on unexpected error
    await emit(
      eventManager,
      buildWorkflowExecutionJoinFailedEvent({
        parentExecutionId: workflowExecutionEntity.id,
        childExecutionIds: completedPathIds,
        joinStrategy: config.joinStrategy,
        error: getErrorOrNew(error),
      }),
    );

    return {
      completedBranches: completedPathIds,
      failedBranches: [],
      skippedBranches: [],
      strategy: config.joinStrategy,
      aggregatedOutput: { error: String(error), executionTime: executionDuration },
    };
  }
}

export type { JoinNodeOutput };
