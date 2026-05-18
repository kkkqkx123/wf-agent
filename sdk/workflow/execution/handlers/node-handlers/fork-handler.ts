/**
 * Fork Node Handler - Executes all fork branches in parallel
 *
 * Creates independent execution entities for each fork path,
 * executes them concurrently, and returns standardized results.
 *
 * Design Principles:
 * - Each branch gets its own isolated WorkflowExecutionEntity
 * - Branches execute in true parallel via Promise.all()
 * - Variables are deep cloned to prevent race conditions
 * - Returns typed ForkBranchResult[] array
 */

import type { RuntimeNode, ForkNodeConfig } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { GlobalContext } from "../../../../core/global-context.js";
import type { ForkBranchResult } from "../../types/subworkflow-result.types.js";
import { createForkBranchResult } from "../../types/subworkflow-result.types.js";
import type { ForkHandlerContext } from "../../types/fork.types.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { emit } from "../../../../core/utils/event/event-emitter.js";
import {
  buildForkStartedEvent,
  buildForkBranchStartedEvent,
  buildForkBranchCompletedEvent,
  buildForkCompletedEvent,
} from "../../utils/event/index.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import { cleanupChildExecution } from "../../utils/child-execution-cleanup.js";

const logger = createContextualLogger({ component: "fork-node-handler" });



/**
 * Check whether the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity): boolean {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return false;
  }
  return true;
}

/**
 * Fork Node Processing Function
 *
 * Orchestrates parallel execution of all fork branches:
 * 1. Creates independent execution entity for each fork path
 * 2. Executes all branches concurrently using Promise.all()
 * 3. Returns ForkBranchResult[] with standardized structure
 *
 * @param globalContext Global application context (for event emission)
 * @param workflowExecutionEntity Parent workflow execution entity
 * @param node FORK runtime node with forkPaths configuration
 * @param context Handler context containing executionBuilder and workflowExecutor
 * @returns Array of ForkBranchResult, one per fork path
 */
export async function forkHandler(
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  context?: ForkHandlerContext,
): Promise<ForkBranchResult[]> {
  // Check if it is possible to execute.
  if (!canExecute(workflowExecutionEntity)) {
    logger.warn("FORK node skipped - workflow not in RUNNING state", {
      nodeId: node.id,
      status: workflowExecutionEntity.getStatus(),
    });
    return [];
  }

  const config = node.config as ForkNodeConfig;
  const forkPaths = config.forkPaths;

  if (!forkPaths || forkPaths.length === 0) {
    throw new Error(`FORK node '${node.id}' must have at least one forkPath`);
  }

  // Validate required dependencies
  const builder = context?.executionBuilder;
  const executor = context?.workflowExecutor;

  if (!builder) {
    throw new Error('WorkflowExecutionBuilder required for FORK execution');
  }
  if (!executor) {
    throw new Error('WorkflowExecutor required for FORK execution');
  }

  logger.info("Starting FORK node execution", {
    nodeId: node.id,
    forkPathCount: forkPaths.length,
    forkStrategy: config.forkStrategy,
  });

  const startTime = now();
  
  // Track branch creations for cleanup in case of failure
  let branchCreations: Array<{ pathId: string; branchEntity: any }> = [];

  try {
    // Step 0: Initialize SyncBarrier for parent execution (REQUIRED for FORK nodes)
    // This must be done BEFORE creating child executions so that path mappings can be registered
    if (!workflowExecutionEntity.hasSyncBarrier()) {
      const eventRegistry = globalContext.container.get(Identifiers.EventRegistry);
      
      if (!eventRegistry) {
        throw new Error(
          'EventRegistry not available in global context. ' +
          'FORK nodes require EventRegistry for SyncBarrier initialization.'
        );
      }
      
      workflowExecutionEntity.initializeSyncBarrier(eventRegistry);
      logger.info("SyncBarrier initialized for FORK node", {
        nodeId: node.id,
        executionId: workflowExecutionEntity.id,
        forkPathCount: forkPaths.length,
      });
    }

    // Emit FORK_STARTED event
    const eventManager = globalContext.container.get(Identifiers.EventRegistry) as any;
    if (eventManager) {
      await emit(eventManager, buildForkStartedEvent({
        executionId: workflowExecutionEntity.id,
        workflowId: workflowExecutionEntity.getWorkflowId(),
        nodeId: node.id,
        branchCount: forkPaths.length,
      }));
    }

    // Step 1: Create all branch execution entities in parallel
    logger.debug("Creating fork branch execution entities", {
      nodeId: node.id,
      branchCount: forkPaths.length,
    });

    branchCreations = await Promise.all(
      forkPaths.map(async (path) => {
        const buildResult = await builder.createChildExecution(workflowExecutionEntity, {
          type: 'FORK_BRANCH',
          config: {
            forkPathId: path.pathId,
            startNodeId: path.childNodeId,
          },
        });

        logger.debug("Fork branch entity created", {
          nodeId: node.id,
          forkPathId: path.pathId,
          branchExecutionId: buildResult.workflowExecutionEntity.id,
        });

        // Register path-to-execution mapping in SyncBarrier
        const syncBarrier = workflowExecutionEntity.getSyncBarrier();
        if (syncBarrier) {
          syncBarrier.registerPath(path.pathId, buildResult.workflowExecutionEntity.id);
          logger.debug("Registered fork path in SyncBarrier", {
            forkPathId: path.pathId,
            executionId: buildResult.workflowExecutionEntity.id,
          });
        }

        return {
          pathId: path.pathId,
          branchEntity: buildResult.workflowExecutionEntity,
        };
      })
    );

    // Emit FORK_BRANCH_STARTED events for each branch
    if (eventManager) {
      for (const branch of branchCreations) {
        await emit(eventManager, buildForkBranchStartedEvent({
          executionId: workflowExecutionEntity.id,
          workflowId: workflowExecutionEntity.getWorkflowId(),
          nodeId: node.id,
          forkPathId: branch.pathId,
          branchExecutionId: branch.branchEntity.id,
        }));
      }
    }

    // Step 2: Execute all branches in parallel
    logger.debug("Executing fork branches in parallel", {
      nodeId: node.id,
      branchCount: branchCreations.length,
    });

    const executionResults = await Promise.all(
      branchCreations.map(async (branch) => {
        const branchStartTime = now();
        const result = await executor.executeWorkflow(branch.branchEntity);
        const branchDuration = diffTimestamp(branchStartTime, now());

        logger.debug("Fork branch completed", {
          nodeId: node.id,
          forkPathId: branch.pathId,
          branchExecutionId: branch.branchEntity.id,
          duration: branchDuration,
          status: result.metadata.status,
        });

        return {
          ...result,
          executionTime: branchDuration,
        };
      })
    );

    // Step 3: Build ForkBranchResult array with cleanup
    const results: ForkBranchResult[] = await Promise.all(
      branchCreations.map(async (branch, index) => {
        const executionResult = executionResults[index];
        if (!executionResult) {
          throw new Error(`Missing execution result for branch ${branch.pathId}`);
        }
        
        // Cleanup branch execution entity after completion
        await cleanupChildExecution(
          branch.branchEntity,
          workflowExecutionEntity,
          executionResult.metadata.status === 'COMPLETED' ? 'COMPLETED' : 'FAILED'
        );
        
        return createForkBranchResult(
          branch.pathId,
          branch.branchEntity,
          executionResult,
          executionResult.executionTime
        );
      })
    );

    // Emit FORK_BRANCH_COMPLETED events for each branch
    if (eventManager) {
      results.forEach(result => {
        emit(eventManager, buildForkBranchCompletedEvent({
          executionId: workflowExecutionEntity.id,
          workflowId: workflowExecutionEntity.getWorkflowId(),
          nodeId: node.id,
          forkPathId: result.forkPathId,
          branchExecutionId: result.branchEntity.id,
          status: result.executionResult.metadata.status,
          executionTime: result.executionTime,
        }));
      });
    }

    const totalDuration = diffTimestamp(startTime, now());

    // Record FORK metrics
    try {
      const metricsRegistry = globalContext.container.get(Identifiers.MetricsRegistry);
      const nodeCollector = metricsRegistry?.getNodeCollector();
      if (nodeCollector && typeof nodeCollector.recordForkExecution === 'function') {
        const branchDurations = results.map(r => r.executionTime);
        nodeCollector.recordForkExecution(node.id, workflowExecutionEntity.getWorkflowId(), {
          branchCount: forkPaths.length,
          totalDuration,
          successCount: results.filter(r => r.executionResult.metadata.status === 'COMPLETED').length,
          failureCount: results.filter(r => r.executionResult.metadata.status === 'FAILED').length,
          maxBranchDuration: Math.max(...branchDurations),
          minBranchDuration: Math.min(...branchDurations),
        });

        // Record individual branch metrics
        results.forEach(result => {
          if (typeof nodeCollector.recordForkBranchExecution === 'function') {
            nodeCollector.recordForkBranchExecution({
              nodeId: node.id,
              forkPathId: result.forkPathId,
              duration: result.executionTime,
              status: result.executionResult.metadata.status,
            });
          }
        });
      }
    } catch (metricsError) {
      logger.warn("Failed to record fork metrics", { error: getErrorOrNew(metricsError) });
    }

    // Emit FORK_COMPLETED event
    if (eventManager) {
      await emit(eventManager, buildForkCompletedEvent({
        executionId: workflowExecutionEntity.id,
        workflowId: workflowExecutionEntity.getWorkflowId(),
        nodeId: node.id,
        totalBranches: results.length,
        successCount: results.filter(r => r.executionResult.metadata.status === 'COMPLETED').length,
        failureCount: results.filter(r => r.executionResult.metadata.status === 'FAILED').length,
        totalExecutionTime: totalDuration,
      }));
    }

    logger.info("FORK node execution completed", {
      nodeId: node.id,
      branchCount: results.length,
      totalDuration,
      successCount: results.filter(r => r.executionResult.metadata.status === 'COMPLETED').length,
      failureCount: results.filter(r => r.executionResult.metadata.status === 'FAILED').length,
    });

    return results;
  } catch (error) {
    const errorObj = error instanceof Error ? error : new Error(String(error));
    const totalDuration = diffTimestamp(startTime, now());

    logger.error("FORK node execution failed", {
      nodeId: node.id,
      error: errorObj,
      duration: totalDuration,
    });

    // Cleanup any created branch entities on failure
    // Track which branches were successfully created before the error
    if (branchCreations && branchCreations.length > 0) {
      logger.debug("Cleaning up fork branches due to failure", {
        nodeId: node.id,
        branchCount: branchCreations.length,
      });
      
      // Clean up all successfully created branches
      await Promise.all(
        branchCreations.map(async (branch) => {
          try {
            await cleanupChildExecution(
              branch.branchEntity,
              workflowExecutionEntity,
              'FAILED'
            );
          } catch (cleanupError) {
            logger.warn("Failed to cleanup fork branch", {
              nodeId: node.id,
              forkPathId: branch.pathId,
              branchExecutionId: branch.branchEntity.id,
              cleanupError,
            });
          }
        })
      );
    }

    throw errorObj;
  }
}
