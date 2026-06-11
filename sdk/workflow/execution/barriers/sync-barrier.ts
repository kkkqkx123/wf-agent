/**
 * SyncBarrier - Synchronization Barrier for Fork/Join Execution
 * 
 * Manages cross-branch synchronization by tracking fork path to execution ID mappings
 * and providing event-driven waiting mechanisms.
 * 
 * Design:
 * - Each parent workflow execution has one SyncBarrier instance
 * - Fork handler registers path-to-execution mappings when creating branches
 * - SYNC nodes use the barrier to wait for sibling branch completion
 * - Join nodes can query barrier status for join condition evaluation
 */

import type { ID } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { ExecutionHierarchyRegistry } from "../../../core/registry/execution-hierarchy-registry.js";
import { waitForWorkflowExecutionCompleted, WAIT_FOREVER } from "../utils/event/event-waiter.js";
import { isTimeoutError } from "../../../core/utils/timeout/timeout-utils.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "SyncBarrier" });

/**
 * Result of waiting for multiple branches via SyncBarrier.waitForMultipleBranches
 */
export interface WaitForMultipleResult {
  successful: Map<ID, WorkflowExecutionEntity>;
  failed: Array<{ pathId: ID; error: unknown }>;
  totalRequested: number;
}

/**
 * SyncBarrier - Manages fork path to execution ID mappings and provides waiting mechanisms
 */
export class SyncBarrier {
  /** Fork path ID to execution ID mapping */
  private pathToExecutionMap: Map<ID, ID> = new Map();

  /** Execution ID to path ID reverse mapping (for quick lookup) */
  private executionToPathMap: Map<ID, ID> = new Map();
  
  /** Parent execution ID (the execution that owns this barrier) */
  private readonly parentExecutionId: ID;
  
  /** Event registry for cross-execution event listening */
  private readonly eventManager: EventRegistry;
  
  /** Execution hierarchy registry to get execution entities */
  private readonly executionRegistry?: ExecutionHierarchyRegistry;

  constructor(
    parentExecutionId: ID,
    eventManager: EventRegistry,
    executionRegistry?: ExecutionHierarchyRegistry
  ) {
    this.parentExecutionId = parentExecutionId;
    this.eventManager = eventManager;
    this.executionRegistry = executionRegistry;
    
    logger.debug("SyncBarrier initialized", {
      parentExecutionId,
    });
  }

  /**
   * Register a fork path to execution ID mapping
   * Called by fork handler when creating child executions
   * 
   * @param forkPathId The fork path ID
   * @param executionId The child execution ID
   */
  registerPath(forkPathId: ID, executionId: ID): void {
    if (this.pathToExecutionMap.has(forkPathId)) {
      const oldExecutionId = this.pathToExecutionMap.get(forkPathId);
      // Clean up old reverse mapping before overwriting
      if (oldExecutionId !== undefined) {
        this.executionToPathMap.delete(oldExecutionId);
      }
      logger.warn("Fork path already registered, overwriting", {
        forkPathId,
        existingExecutionId: oldExecutionId,
        newExecutionId: executionId,
      });
    }

    this.pathToExecutionMap.set(forkPathId, executionId);
    this.executionToPathMap.set(executionId, forkPathId);

    logger.debug("Registered fork path mapping", {
      forkPathId,
      executionId,
      totalPaths: this.pathToExecutionMap.size,
    });
  }

  /**
   * Get execution ID by fork path ID
   * 
   * @param forkPathId The fork path ID
   * @returns Execution ID or undefined if not found
   */
  getExecutionIdByPath(forkPathId: ID): ID | undefined {
    return this.pathToExecutionMap.get(forkPathId);
  }

  /**
   * Get fork path ID by execution ID
   * 
   * @param executionId The execution ID
   * @returns Fork path ID or undefined if not found
   */
  getPathByExecutionId(executionId: ID): ID | undefined {
    return this.executionToPathMap.get(executionId);
  }

  /**
   * Get all registered fork path IDs
   * 
   * @returns Array of fork path IDs
   */
  getAllPathIds(): ID[] {
    return Array.from(this.pathToExecutionMap.keys());
  }

  /**
   * Get all registered execution IDs
   * 
   * @returns Array of execution IDs
   */
  getAllExecutionIds(): ID[] {
    return Array.from(this.pathToExecutionMap.values());
  }

  /**
   * Check if a fork path is registered
   * 
   * @param forkPathId The fork path ID
   * @returns True if registered
   */
  hasPath(forkPathId: ID): boolean {
    return this.pathToExecutionMap.has(forkPathId);
  }

  /**
   * Wait for a specific fork branch to complete
   * Uses event-driven waiting with optional timeout
   * 
   * @param forkPathId The fork path ID to wait for
   * @param timeout Timeout in seconds (0 = no timeout, wait indefinitely)
   * @returns The completed workflow execution entity
   * @throws Error if timeout exceeded or execution failed
   */
  async waitForBranchCompletion(
    forkPathId: ID,
    timeout: number = 0
  ): Promise<WorkflowExecutionEntity> {
    const executionId = this.pathToExecutionMap.get(forkPathId);
    
    if (!executionId) {
      throw new Error(`Fork path not registered: ${forkPathId}`);
    }

    logger.debug("Waiting for branch completion", {
      forkPathId,
      executionId,
      timeout: timeout > 0 ? `${timeout}s` : "infinite",
    });

    try {
      // Convert seconds to milliseconds for event waiter.
      // timeout=0 means no timeout (wait indefinitely).
      const timeoutMs = timeout > 0 ? timeout * 1000 : WAIT_FOREVER;

      await waitForWorkflowExecutionCompleted(this.eventManager, executionId, timeoutMs);

      // Get the execution entity after waiting completes
      const entity = this.executionRegistry?.get(executionId);
      if (!entity) {
        throw new Error(`Failed to get execution entity for executionId: ${executionId}`);
      }
      
      const result = entity as WorkflowExecutionEntity;

      logger.debug("Branch completed successfully", {
        forkPathId,
        executionId,
        status: result.getStatus(),
      });

      return result;
    } catch (error) {
      if (isTimeoutError(error)) {
        logger.error("Branch completion timed out", {
          forkPathId,
          executionId,
          timeout,
        });
        throw error;
      }
      
      logger.error("Error waiting for branch completion", {
        forkPathId,
        executionId,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  }

  /**
   * Wait for multiple fork branches to complete
   * Useful for implementing custom join strategies
   *
   * @param forkPathIds Array of fork path IDs to wait for
   * @param timeout Timeout in seconds per branch (0 = no timeout)
   * @returns Object with successful and failed branch results
   */
  async waitForMultipleBranches(
    forkPathIds: ID[],
    timeout: number = 0
  ): Promise<WaitForMultipleResult> {
    const successful = new Map<ID, WorkflowExecutionEntity>();
    const failed: Array<{ pathId: ID; error: unknown }> = [];

    logger.debug("Waiting for multiple branches", {
      forkPathIds,
      count: forkPathIds.length,
    });

    // Wait for all branches concurrently
    const promises = forkPathIds.map(async (pathId) => {
      try {
        const execution = await this.waitForBranchCompletion(pathId, timeout);
        return { pathId, execution, success: true };
      } catch (error) {
        logger.error("Failed to wait for branch", {
          pathId,
          error: error instanceof Error ? error.message : String(error),
        });
        return { pathId, execution: null, success: false, error };
      }
    });

    const settled = await Promise.allSettled(promises);

    settled.forEach((result) => {
      if (result.status === "fulfilled" && result.value.success) {
        successful.set(result.value.pathId, result.value.execution!);
      } else if (result.status === "fulfilled" && !result.value.success) {
        failed.push({ pathId: result.value.pathId, error: result.value.error });
      }
    });

    logger.debug("Multiple branches wait completed", {
      totalRequested: forkPathIds.length,
      successful: successful.size,
      failed: failed.length,
    });

    return { successful, failed, totalRequested: forkPathIds.length };
  }

  /**
   * Clear all mappings (called when parent execution completes)
   */
  clear(): void {
    const pathCount = this.pathToExecutionMap.size;
    this.pathToExecutionMap.clear();
    this.executionToPathMap.clear();

    logger.debug("SyncBarrier cleared", {
      parentExecutionId: this.parentExecutionId,
      clearedPaths: pathCount,
    });
  }

  /**
   * Get barrier statistics for monitoring/debugging
   */
  getStats(): {
    totalPaths: number;
    parentExecutionId: ID;
    paths: Array<{ forkPathId: ID; executionId: ID }>;
  } {
    const paths = Array.from(this.pathToExecutionMap.entries()).map(
      ([forkPathId, executionId]) => ({ forkPathId, executionId })
    );
    
    return {
      totalPaths: this.pathToExecutionMap.size,
      parentExecutionId: this.parentExecutionId,
      paths,
    };
  }
}
