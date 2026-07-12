/**
 * Fork Node Handler - Executes all fork branches in parallel
 *
 * Creates independent execution entities for each fork path,
 * executes them concurrently, and returns standardized results.
 *
 * Design Principles:
 * - Each branch gets its own isolated WorkflowExecutionEntity
 * - Branches execute in true parallel via Promise.allSettled
 * - A single branch failure does NOT prevent other branches from completing
 * - Failure strategy (fail-fast / continue-on-error / fail-on-threshold) controls overall behavior
 * - Variables are deep cloned to prevent race conditions
 * - Returns typed ForkBranchResult[] array
 */

import type { RuntimeNode, ForkNodeConfig } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { GlobalContext } from "../../../../shared/global-context.js";
import type { ForkBranchResult } from "../../types/subworkflow-result.types.js";
import { createForkBranchResult } from "../../types/subworkflow-result.types.js";
import type { ForkHandlerContext } from "../../types/fork.types.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import { emit } from "../../../../shared/utils/event/emit-event.js";
import {
  buildForkStartedEvent,
  buildForkBranchStartedEvent,
  buildForkBranchCompletedEvent,
  buildForkCompletedEvent,
} from "../../../../shared/utils/event/builders/index.js";
import * as Identifiers from "../../../../di/service-identifiers.js";
import { cleanupChildExecution } from "../../utils/child-execution-cleanup.js";

/**
 * Internal result type for a single fork branch execution,
 * holding either success or failure data before the final ForkBranchResult is built.
 */
interface BranchExecutionOutcome {
  pathId: string;
  branchEntity: WorkflowExecutionEntity;
  status: "COMPLETED" | "FAILED";
  executionResult?: import("@wf-agent/types").WorkflowExecutionResult;
  error?: Error;
  executionTime: number;
}

const logger = createContextualLogger({ component: "fork-node-handler" });

/**
 * Helper function to execute a branch with retry support and timeout
 *
 * Implements exponential backoff retry with global retry budget consumption.
 * Each branch execution respects per-branch timeout if configured.
 *
 * Problem #4 Fix: Support per-branch budget allocation
 * Problem #5 Fix: Support time budget modes (delay-only or total-time)
 * Problem #8 Fix: Support both perExecutionTimeout and totalBranchTimeout
 *
 * @param pathId Fork path ID
 * @param branchEntity Branch execution entity
 * @param nodeId FORK node ID
 * @param config FORK node configuration
 * @param executor Workflow executor
 * @param parentEntity Parent workflow execution entity
 * @returns Branch execution outcome with retry attempt tracking
 */
async function executeBranchWithRetry(
  pathId: string,
  branchEntity: WorkflowExecutionEntity,
  nodeId: string,
  config: ForkNodeConfig,
  executor: any, // WorkflowExecutor type
  parentEntity: WorkflowExecutionEntity,
): Promise<BranchExecutionOutcome> {
  const retryPolicy = config.retryPolicy;
  const perExecutionTimeout = config.childExecutionTimeout; // Problem #8: renamed for clarity
  const totalBranchTimeout = config.totalBranchTimeout; // Problem #8: new field
  const retryBudget = parentEntity.getRetryBudget();

  let lastError: Error | undefined;
  let attempts = 0;
  const totalStartTime = now();
  const maxAttempts = retryPolicy?.enabled ? (retryPolicy.maxRetries ?? 0) + 1 : 1;

  while (attempts < maxAttempts) {
    const branchStartTime = now();

    try {
      // Check total branch timeout (Problem #8)
      if (totalBranchTimeout && totalBranchTimeout > 0) {
        const elapsedMs = diffTimestamp(totalStartTime, now());
        if (elapsedMs > totalBranchTimeout) {
          throw new Error(
            `Branch total timeout exceeded after ${elapsedMs}ms (limit: ${totalBranchTimeout}ms)`,
          );
        }
      }

      // Execute branch with optional per-execution timeout (Problem #8)
      let result: import("@wf-agent/types").WorkflowExecutionResult;

      if (perExecutionTimeout && perExecutionTimeout > 0) {
        // Apply per-execution timeout using Promise.race with proper cleanup (Problem #3 fix)
        let timeoutId: NodeJS.Timeout | undefined;
        const timeoutPromise = new Promise<never>((_, reject) => {
          timeoutId = setTimeout(
            () => reject(new Error(`Branch execution timeout after ${perExecutionTimeout}ms`)),
            perExecutionTimeout,
          );
        });

        try {
          result = await Promise.race([executor.executeWorkflow(branchEntity), timeoutPromise]);
          // Clear timeout on successful completion
          if (timeoutId) clearTimeout(timeoutId);
        } catch (error) {
          // Clear timeout on error
          if (timeoutId) clearTimeout(timeoutId);

          // If timeout occurred, stop the branch entity to ensure cleanup
          if (error instanceof Error && error.message.includes("timeout")) {
            logger.debug("Timeout occurred, stopping branch execution", {
              nodeId,
              forkPathId: pathId,
              branchExecutionId: branchEntity.id,
            });
            try {
              branchEntity.stop();
            } catch (stopError) {
              logger.warn("Failed to stop branch on timeout", {
                nodeId,
                forkPathId: pathId,
                stopError,
              });
            }
          }

          throw error;
        }
      } else {
        result = await executor.executeWorkflow(branchEntity);
      }

      const branchDuration = diffTimestamp(branchStartTime, now());

      logger.debug("Fork branch completed successfully", {
        nodeId,
        forkPathId: pathId,
        branchExecutionId: branchEntity.id,
        duration: branchDuration,
        attempts: attempts + 1,
        status: result.metadata.status,
      });

      return {
        pathId,
        branchEntity,
        status: "COMPLETED" as const,
        executionResult: result,
        executionTime: branchDuration,
      };
    } catch (error) {
      lastError = error instanceof Error ? error : new Error(String(error));
      attempts++;
      const executionDurationMs = diffTimestamp(branchStartTime, now());
      const branchDuration = diffTimestamp(totalStartTime, now());

      logger.warn("Fork branch execution failed", {
        nodeId,
        forkPathId: pathId,
        branchExecutionId: branchEntity.id,
        duration: branchDuration,
        attemptDuration: executionDurationMs,
        attempt: attempts,
        maxAttempts,
        error: lastError,
      });

      // Check if should retry (Task #7)
      if (
        attempts < maxAttempts &&
        retryPolicy?.enabled &&
        retryPolicy.shouldRetry(lastError, attempts - 1)
      ) {
        // Calculate delay for exponential backoff
        const delay = retryPolicy.getNextDelay(attempts - 1);

        logger.debug("Branch retry scheduled", {
          nodeId,
          forkPathId: pathId,
          branchExecutionId: branchEntity.id,
          attemptNumber: attempts,
          nextDelay: delay,
        });

        // Check global retry budget with per-branch allocation (Problem #4) and time budget (Problem #5)
        if (retryBudget) {
          // Determine time budget mode (Problem #5)
          const timeBudgetMode = retryPolicy.timeBudgetMode ?? 'delay-only';
          const executionTimeForBudget =
            timeBudgetMode === 'total-time' ? executionDurationMs : 0;

          // Check budget with branch ID (Problem #4) and execution time (Problem #5)
          if (!retryBudget.canRetry(delay, pathId, executionTimeForBudget)) {
            logger.warn("Retry budget exhausted (Problem #4), stopping retries", {
              nodeId,
              forkPathId: pathId,
              retriesRemaining: retryBudget.getRetriesRemaining(),
              branchBudget: retryBudget.getBranchBudgetState(pathId),
            });
            break;
          }

          // Consume from retry budget
          retryBudget.consumeRetry(delay, pathId, executionTimeForBudget);
        }

        // Wait before retry
        await new Promise(resolve => setTimeout(resolve, delay));
      } else {
        // No more retries
        break;
      }
    }
  }

  // Return failed outcome after all retry attempts exhausted
  const totalExecutionTime = diffTimestamp(totalStartTime, now());

  // Check if total branch timeout was exceeded (Problem #8)
  if (totalBranchTimeout && totalBranchTimeout > 0 && totalExecutionTime > totalBranchTimeout) {
    logger.warn("Branch exceeded total timeout (Problem #8)", {
      nodeId,
      forkPathId: pathId,
      totalExecutionTime,
      totalBranchTimeout,
    });
  }

  return {
    pathId,
    branchEntity,
    status: "FAILED" as const,
    error: lastError,
    executionTime: totalExecutionTime,
  };
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
): Promise<{ launchedBranches: ForkBranchResult[] }> {
  const config = node.config as ForkNodeConfig;
  const forkPaths = config.forkPaths;

  if (!forkPaths || forkPaths.length === 0) {
    throw new Error(`FORK node '${node.id}' must have at least one forkPath`);
  }

  // Validate required dependencies
  const builder = context?.executionBuilder;
  const executor = context?.workflowExecutor;

  if (!builder) {
    throw new Error("WorkflowExecutionBuilder required for FORK execution");
  }
  if (!executor) {
    throw new Error("WorkflowExecutor required for FORK execution");
  }

  // Extract failure strategy from config (default: fail-fast)
      const failureStrategy = config.failureStrategy ?? "fail-fast";
      const maxFailedBranches = config.maxFailedBranches ?? 0;

      logger.info("Starting FORK node execution", {
        nodeId: node.id,
        forkPathCount: forkPaths.length,
        forkStrategy: config.forkStrategy,
        failureStrategy,
        maxFailedBranches,
      });

      const startTime = now();

  // Track branch creations for cleanup in case of failure
  let branchCreations: Array<{ pathId: string; branchEntity: WorkflowExecutionEntity }> = [];

  try {
    // Step 0: Initialize SyncBarrier for parent execution (REQUIRED for FORK nodes)
    // This must be done BEFORE creating child executions so that path mappings can be registered
    if (!workflowExecutionEntity.hasSyncBarrier()) {
      const eventRegistry = globalContext.container.get(Identifiers.EventRegistry) as
        | EventRegistry
        | undefined;

      if (!eventRegistry) {
        throw new Error(
          "EventRegistry not available in global context. " +
            "FORK nodes require EventRegistry for SyncBarrier initialization.",
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
    const eventManager = globalContext.container.get(Identifiers.EventRegistry) as
      | EventRegistry
      | undefined;
    if (eventManager) {
      await emit(
        eventManager,
        buildForkStartedEvent({
          executionId: workflowExecutionEntity.id,
          workflowId: workflowExecutionEntity.getWorkflowId(),
          nodeId: node.id,
          branchCount: forkPaths.length,
        }),
      );
    }

    // Step 1: Create all branch execution entities in parallel
    logger.debug("Creating fork branch execution entities", {
      nodeId: node.id,
      branchCount: forkPaths.length,
    });

    branchCreations = await Promise.all(
      forkPaths.map(async path => {
        const buildResult = await builder.createChildExecution(workflowExecutionEntity, {
          type: "FORK_BRANCH",
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
      }),
    );

    // Emit FORK_BRANCH_STARTED events for each branch
    if (eventManager) {
      for (const branch of branchCreations) {
        await emit(
          eventManager,
          buildForkBranchStartedEvent({
            executionId: workflowExecutionEntity.id,
            workflowId: workflowExecutionEntity.getWorkflowId(),
            nodeId: node.id,
            forkPathId: branch.pathId,
            branchExecutionId: branch.branchEntity.id,
          }),
        );
      }
    }

    // Problem #4 Fix: Allocate per-branch retry budgets BEFORE executing branches
    // This prevents starvation where fast-failing branches consume all budget
    const retryBudget = workflowExecutionEntity.getRetryBudget();
    if (retryBudget && config.retryPolicy?.enabled) {
      const branchIds = branchCreations.map(b => b.pathId);
      const allocatedPerBranch = retryBudget.allocateBranchBudgets(branchIds);
      logger.info("Fork branch budgets allocated (Problem #4)", {
        nodeId: node.id,
        branchCount: branchIds.length,
        allocatedPerBranch,
      });
    }

    // Step 2: Execute all branches in parallel with isolation
    // Each branch is wrapped in try/catch so a single branch failure
    // does NOT prevent other branches from completing.
    logger.debug("Executing fork branches in parallel", {
      nodeId: node.id,
      branchCount: branchCreations.length,
      failureStrategy,
      retryPolicyEnabled: config.retryPolicy?.enabled ?? false,
      childExecutionTimeout: config.childExecutionTimeout,
      totalBranchTimeout: config.totalBranchTimeout,
    });

    const settledResults = await Promise.allSettled(
      branchCreations.map(async branch => {
        return executeBranchWithRetry(
          branch.pathId,
          branch.branchEntity,
          node.id,
          config,
          executor,
          workflowExecutionEntity,
        );
      }),
    );

    // Convert PromiseSettledResult[] to BranchExecutionOutcome[]
    // Ensures all branches are processed regardless of individual failures
    const branchOutcomes: BranchExecutionOutcome[] = settledResults.map((result, index) => {
      const branch = branchCreations[index]!; // Assert non-null as indices match
      if (result.status === "fulfilled") {
        return result.value;
      } else {
        // Handle promise rejection as a failed branch
        const rejectionError = result.reason instanceof Error
          ? result.reason
          : new Error(String(result.reason));
        return {
          pathId: branch.pathId,
          branchEntity: branch.branchEntity,
          status: "FAILED" as const,
          error: rejectionError,
          executionTime: 0, // Unknown duration for rejected promises
        };
      }
    });

    // Apply failure strategy
    const failedCount = branchOutcomes.filter(o => o.status === "FAILED").length;

    if (failureStrategy === "fail-fast" && failedCount > 0) {
      // fail-fast: throw as soon as any branch fails (preserving the existing behavior)
      const firstFailure = branchOutcomes.find(o => o.status === "FAILED")!;
      throw new Error(
        `FORK node '${node.id}' failed (fail-fast): branch '${firstFailure.pathId}' failed - ${firstFailure.error?.message}`,
        { cause: firstFailure.error },
      );
    }

    if (failureStrategy === "fail-on-threshold" && failedCount > maxFailedBranches) {
      throw new Error(
        `FORK node '${node.id}' failed (fail-on-threshold): ${failedCount} branches failed, threshold is ${maxFailedBranches}`,
      );
    }

    // continue-on-error (or threshold met): proceed with all outcomes

    // Step 3: Build ForkBranchResult array with cleanup
    const results: ForkBranchResult[] = await Promise.all(
      branchOutcomes.map(async outcome => {
        // Cleanup branch execution entity
        await cleanupChildExecution(
          outcome.branchEntity,
          workflowExecutionEntity,
          outcome.status === "COMPLETED" ? "COMPLETED" : "FAILED",
        );

        if (outcome.status === "COMPLETED") {
          return createForkBranchResult(
            outcome.pathId,
            outcome.branchEntity,
            outcome.executionResult!,
            outcome.executionTime,
          );
        }

        // For failed branches, build a result with FAILED status
        const failedResult: import("@wf-agent/types").WorkflowExecutionResult = {
          executionId: outcome.branchEntity.id,
          output: outcome.branchEntity.getOutput(),
          executionTime: outcome.executionTime,
          nodeResults: outcome.branchEntity.getNodeResults(),
          metadata: {
            status: "FAILED",
            startTime: outcome.branchEntity.getStartTime() || Date.now(),
            endTime: Date.now(),
            executionTime: outcome.executionTime,
            nodeCount: outcome.branchEntity.getNodeResults().length,
            errorCount: outcome.branchEntity.getErrors().length,
          },
        };

        return createForkBranchResult(
          outcome.pathId,
          outcome.branchEntity,
          failedResult,
          outcome.executionTime,
        );
      }),
    );

    // Emit FORK_BRANCH_COMPLETED events for each branch
    if (eventManager) {
      results.forEach(result => {
        emit(
          eventManager,
          buildForkBranchCompletedEvent({
            executionId: workflowExecutionEntity.id,
            workflowId: workflowExecutionEntity.getWorkflowId(),
            nodeId: node.id,
            forkPathId: result.forkPathId,
            branchExecutionId: result.branchEntity.id,
            status: result.executionResult.metadata.status,
            executionTime: result.executionTime,
          }),
        );
      });
    }

    const totalDuration = diffTimestamp(startTime, now());

    // Record FORK metrics
    try {
      const metricsRegistry = globalContext.container.get(Identifiers.MetricsRegistry);
      const nodeCollector = metricsRegistry?.getNodeCollector();
      if (nodeCollector && typeof nodeCollector.recordForkExecution === "function") {
        const branchDurations = results.map(r => r.executionTime);
        nodeCollector.recordForkExecution(node.id, workflowExecutionEntity.getWorkflowId(), {
          branchCount: forkPaths.length,
          totalDuration,
          successCount: results.filter(r => r.executionResult.metadata.status === "COMPLETED")
            .length,
          failureCount: results.filter(r => r.executionResult.metadata.status === "FAILED").length,
          maxBranchDuration: Math.max(...branchDurations),
          minBranchDuration: Math.min(...branchDurations),
        });

        // Record individual branch metrics
        results.forEach(result => {
          if (typeof nodeCollector.recordForkBranchExecution === "function") {
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
      await emit(
        eventManager,
        buildForkCompletedEvent({
          executionId: workflowExecutionEntity.id,
          workflowId: workflowExecutionEntity.getWorkflowId(),
          nodeId: node.id,
          totalBranches: results.length,
          successCount: results.filter(r => r.executionResult.metadata.status === "COMPLETED")
            .length,
          failureCount: results.filter(r => r.executionResult.metadata.status === "FAILED").length,
          totalExecutionTime: totalDuration,
        }),
      );
    }

    logger.info("FORK node execution completed", {
      nodeId: node.id,
      branchCount: results.length,
      totalDuration,
      successCount: results.filter(r => r.executionResult.metadata.status === "COMPLETED").length,
      failureCount: results.filter(r => r.executionResult.metadata.status === "FAILED").length,
    });

    return { launchedBranches: results };
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
        branchCreations.map(async branch => {
          try {
            await cleanupChildExecution(branch.branchEntity, workflowExecutionEntity, "FAILED");
          } catch (cleanupError) {
            logger.warn("Failed to cleanup fork branch", {
              nodeId: node.id,
              forkPathId: branch.pathId,
              branchExecutionId: branch.branchEntity.id,
              cleanupError,
            });
          }
        }),
      );
    }

    throw errorObj;
  }
}
