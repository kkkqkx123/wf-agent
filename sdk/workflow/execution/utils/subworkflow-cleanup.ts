/**
 * Subworkflow Cleanup Utilities
 * 
 * Shared cleanup logic for all subworkflow types to ensure
 * consistent resource management on failure.
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { ExecutionHierarchyRegistry } from "../../../core/registry/execution-hierarchy-registry.js";
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
  try {
    logger.debug("Cleaning up failed subworkflow", {
      childExecutionId: childEntity.id,
      parentExecutionId: parentEntity.id,
    });

    // Stop execution if running
    childEntity.stop();

    // Unregister from hierarchy registry
    if (registry && typeof registry.unregister === 'function') {
      registry.unregister(childEntity.id);
    }

    // Remove from parent's children list
    parentEntity.unregisterChild(childEntity.id, 'WORKFLOW');

    logger.info("Failed subworkflow cleaned up successfully", {
      childExecutionId: childEntity.id,
    });
  } catch (cleanupError) {
    logger.warn("Failed to cleanup subworkflow after error", {
      childExecutionId: childEntity.id,
      cleanupError,
    });
  }
}
