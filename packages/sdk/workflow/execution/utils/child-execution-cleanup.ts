/**
 * Child Execution Cleanup Utilities
 *
 * Unified cleanup logic for all child execution types (SUBGRAPH, FORK_BRANCH, TRIGGERED).
 * Ensures consistent parent-child relationship management on completion or failure.
 *
 * Design Principles:
 * - Single responsibility: Only handles parent-child relationship cleanup
 * - Type-safe: Uses proper TypeScript types
 * - Idempotent: Safe to call multiple times
 * - Separation of concerns: Global registry cleanup is handled by workflow-lifecycle-coordinator
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "child-execution-cleanup" });

/**
 * Cleanup reason - why the child execution is being cleaned up
 */
export type CleanupReason = "COMPLETED" | "FAILED" | "CANCELLED";

/**
 * Unified child execution cleanup function
 *
 * Performs standard cleanup steps for all child execution types:
 * 1. Stop execution if running
 * 2. Unregister parent-child relationship
 * 3. Log cleanup action
 *
 * Note: Global registry cleanup is handled by workflow-lifecycle-coordinator.cleanupHierarchy()
 * to avoid duplicate cleanup operations.
 *
 * @param childEntity The child execution entity to clean up
 * @param parentEntity The parent execution entity
 * @param reason Why the cleanup is happening (for logging)
 */
export async function cleanupChildExecution(
  childEntity: WorkflowExecutionEntity,
  parentEntity: WorkflowExecutionEntity,
  reason: CleanupReason,
): Promise<void> {
  try {
    logger.debug("Cleaning up child execution", {
      childExecutionId: childEntity.id,
      parentExecutionId: parentEntity.id,
      reason,
    });

    // Step 1: Stop execution if running
    if (childEntity.getStatus() === "RUNNING") {
      childEntity.stop();
      logger.debug("Stopped running child execution", {
        childExecutionId: childEntity.id,
      });
    }

    // Step 2: Unregister parent-child relationship
    unregisterParentChildRelationship(childEntity, parentEntity);

    // Step 3: Log cleanup completion
    logger.info("Child execution cleaned up successfully", {
      childExecutionId: childEntity.id,
      parentExecutionId: parentEntity.id,
      reason,
    });
  } catch (cleanupError) {
    // Don't throw - cleanup errors should not break the main flow
    logger.warn("Failed to cleanup child execution", {
      childExecutionId: childEntity.id,
      parentExecutionId: parentEntity.id,
      reason,
      cleanupError,
    });
  }
}

/**
 * Unregister parent-child relationship
 *
 * Removes the child reference from the parent entity.
 * This ensures the parent no longer tracks this child execution.
 *
 * @param child The child execution entity
 * @param parent The parent execution entity
 */
function unregisterParentChildRelationship(
  child: WorkflowExecutionEntity,
  parent: WorkflowExecutionEntity,
): void {
  const parentContext = child.getParentContext();

  if (parentContext) {
    // Remove child reference from parent
    parent.unregisterChild(child.id, "WORKFLOW");

    logger.debug("Unregistered parent-child relationship", {
      childExecutionId: child.id,
      parentExecutionId: parent.id,
    });
  } else {
    logger.debug("No parent context found, skipping relationship cleanup", {
      childExecutionId: child.id,
    });
  }
}
