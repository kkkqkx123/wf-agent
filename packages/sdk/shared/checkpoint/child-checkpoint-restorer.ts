/**
 * Child Checkpoint Restorer
 *
 * Generic restorer for recursively restoring child execution instances from checkpoints.
 * Provides symmetric restoration logic for both Workflow and Agent execution hierarchies.
 *
 * Design Principles:
 * - Symmetric: identical restoration logic for both Workflow and Agent
 * - Composable: accepts restoration dependencies via interface
 * - Isolated: one child failure doesn't block others
 *
 * Restores children in order: WORKFLOW before AGENT_LOOP.
 */

import type {
  ChildExecutionReference,
  ExecutionType,
  ID,
} from "@wf-agent/types";
import type { IExecutionEntity } from "../../shared/types/execution-entity.js";

/**
 * Restoration result for a single child
 */
export interface ChildRestoreResult {
  childId: ID;
  childType: ExecutionType;
  success: boolean;
  entity?: IExecutionEntity;
  error?: string;
}

/**
 * Dependencies for child checkpoint restoration
 */
export interface ChildRestoreDependencies {
  /**
   * Find the latest checkpoint ID for a child execution
   * @param childId Child execution ID
   * @param childType Child execution type
   * @returns Checkpoint ID or undefined if not found
   */
  findCheckpoint: (childId: ID, childType: ExecutionType) => Promise<ID | undefined>;

  /**
   * Restore a child execution entity from checkpoint
   * @param checkpointId Checkpoint ID
   * @param childType Child execution type
   * @param parentId Parent execution ID
   * @returns Restored entity
   */
  restoreEntity: (
    checkpointId: ID,
    childType: ExecutionType,
    parentId: ID,
  ) => Promise<IExecutionEntity>;

  /**
   * Register a restored child with its parent
   * @param parent Parent entity
   * @param child Child entity
   * @param childRef Original child reference
   */
  registerChild: (
    parent: IExecutionEntity,
    child: IExecutionEntity,
    childRef: ChildExecutionReference,
  ) => void;

  /**
   * Optional: post-registration hook (e.g., file checkpoint restore)
   */
  onChildRestored?: (child: IExecutionEntity) => Promise<void>;
}

/**
 * Restorer for child execution instances.
 * Manages the recursive restoration of child hierarchies with:
 * - Proper ordering: WORKFLOW children restored before AGENT_LOOP
 * - Error isolation: one child failure doesn't block others
 * - Cycle detection: prevents infinite loops in corrupted hierarchies
 */
export class ChildCheckpointRestorer {
  /**
   * Restore all children of an entity recursively.
   * Restoration order: WORKFLOW children first, then AGENT_LOOP children.
   *
   * @param parentEntity Parent entity whose children to restore
   * @param childRefs Child references from hierarchy metadata
   * @param deps Restoration dependencies
   * @param visited Set of already-visited entity IDs (for cycle detection)
   * @returns Array of restoration results
   */
  async restoreChildren(
    parentEntity: IExecutionEntity,
    childRefs: ChildExecutionReference[],
    deps: ChildRestoreDependencies,
    visited: Set<ID> = new Set([parentEntity.id]),
  ): Promise<ChildRestoreResult[]> {
    const results: ChildRestoreResult[] = [];

    if (childRefs.length === 0) {
      return results;
    }

    // Group by type and restore WORKFLOW first
    const workflowChildren = childRefs.filter(c => c.childType === "WORKFLOW");
    const agentChildren = childRefs.filter(c => c.childType === "AGENT_LOOP");
    const orderedChildren = [...workflowChildren, ...agentChildren];

    for (const childRef of orderedChildren) {
      if (visited.has(childRef.childId)) {
        results.push({
          childId: childRef.childId,
          childType: childRef.childType,
          success: false,
          error: "Cycle detected: already visited",
        });
        continue;
      }

      visited.add(childRef.childId);

      const result = await this.restoreSingleChild(
        parentEntity,
        childRef,
        deps,
        visited,
      );
      results.push(result);
    }

    return results;
  }

  /**
   * Restore a single child and its descendants.
   */
  private async restoreSingleChild(
    parentEntity: IExecutionEntity,
    childRef: ChildExecutionReference,
    deps: ChildRestoreDependencies,
    visited: Set<ID>,
  ): Promise<ChildRestoreResult> {
    try {
      const checkpointId = await deps.findCheckpoint(childRef.childId, childRef.childType);

      if (!checkpointId) {
        return {
          childId: childRef.childId,
          childType: childRef.childType,
          success: false,
          error: "No checkpoint found",
        };
      }

      const childEntity = await deps.restoreEntity(
        checkpointId,
        childRef.childType,
        parentEntity.id,
      );

      childEntity.setParentContext({
        parentType: parentEntity.instanceType === "workflowExecution" ? "WORKFLOW" : "AGENT_LOOP",
        parentId: parentEntity.id,
      });

      deps.registerChild(parentEntity, childEntity, childRef);

      if (deps.onChildRestored) {
        await deps.onChildRestored(childEntity);
      }

      // Recursively restore grandchildren
      const grandChildRefs = childEntity.getChildReferences();
      if (grandChildRefs.length > 0) {
        await this.restoreChildren(childEntity, grandChildRefs, deps, visited);
      }

      return {
        childId: childRef.childId,
        childType: childRef.childType,
        success: true,
        entity: childEntity,
      };
    } catch (error) {
      return {
        childId: childRef.childId,
        childType: childRef.childType,
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Create a summary of restoration results
   */
  static summarizeResults(results: ChildRestoreResult[]): {
    total: number;
    succeeded: number;
    failed: number;
    failures: Array<{ childId: ID; childType: ExecutionType; error: string }>;
  } {
    const failures = results
      .filter(r => !r.success)
      .map(r => ({
        childId: r.childId,
        childType: r.childType,
        error: r.error ?? "Unknown error",
      }));

    return {
      total: results.length,
      succeeded: results.filter(r => r.success).length,
      failed: failures.length,
      failures,
    };
  }
}
