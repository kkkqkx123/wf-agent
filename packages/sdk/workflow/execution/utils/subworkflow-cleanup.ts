/**
 * Subworkflow Cleanup Utilities
 *
 * Shared cleanup logic for all subworkflow types to ensure
 * consistent resource management on failure.
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { ExecutionHierarchyRegistry } from "../../../shared/registry/execution-hierarchy-registry.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "subworkflow-cleanup" });

/**
 * Cleanup failed subworkflow execution
 *
 * Performs standard cleanup steps:
 * 1. Stop execution if running
 * 2. Unregister from hierarchy registry
 * 3. Remove from parent's children list
 *
 * @param childEntity The failed child execution entity
 * @param parentEntity The parent execution entity
 * @param registry The hierarchy registry
 */
export async function cleanupFailedSubworkflow(
  childEntity: WorkflowExecutionEntity,
  parentEntity: WorkflowExecutionEntity,
  registry: ExecutionHierarchyRegistry,
): Promise<void> {
  logger.debug("Cleaning up failed subworkflow", {
    childExecutionId: childEntity.id,
    parentExecutionId: parentEntity.id,
  });

  // Step 1: Stop execution if running (independent try-catch)
  try {
    childEntity.stop();
  } catch (stopError) {
    logger.warn("Failed to stop child execution", {
      childExecutionId: childEntity.id,
      stopError,
    });
  }

  // Step 2: Unregister from hierarchy registry (independent try-catch)
  try {
    if (registry && typeof registry.unregister === "function") {
      registry.unregister(childEntity.id);
    }
  } catch (unregisterError) {
    logger.warn("Failed to unregister from hierarchy registry", {
      childExecutionId: childEntity.id,
      unregisterError,
    });
  }

  // Step 3: Remove from parent's children list (independent try-catch)
  try {
    parentEntity.unregisterChild(childEntity.id, "WORKFLOW");
  } catch (removeError) {
    logger.warn("Failed to remove child from parent's children list", {
      childExecutionId: childEntity.id,
      parentExecutionId: parentEntity.id,
      removeError,
    });
  }

  logger.info("Failed subworkflow cleanup completed", {
    childExecutionId: childEntity.id,
  });
}
