/**
 * Execution Restore Coordinator
 *
 * Coordinates cross-type checkpoint restoration for hierarchical execution trees.
 * Ensures correct restore order: parent before children, workflow before agent.
 * Handles mixed hierarchy scenarios (Workflow->Agent->Workflow, etc.).
 */

import type { ExecutionType, ExecutionHierarchyMetadata, ChildExecutionReference } from "@wf-agent/types";
import type { ID, AgentLoopRuntimeConfig } from "@wf-agent/types";
import type { AnyExecutionEntity, ExecutionHierarchyRegistry } from "../registry/execution-hierarchy-registry.js";
import type { HierarchyValidationResult } from "../execution/hierarchy-integrity-service.js";
import { HierarchyIntegrityService } from "../execution/hierarchy-integrity-service.js";
import type { CheckpointCoordinator, WorkflowCheckpointDependencies } from "../../workflow/checkpoint/checkpoint-coordinator.js";
import type { AgentLoopCheckpointCoordinator, CheckpointDependencies as AgentCheckpointDependencies } from "../../agent/checkpoint/checkpoint-coordinator.js";
import type { FileCheckpointManager } from "@wf-agent/common-utils";

export interface RestoreDependencies {
  /** Workflow checkpoint coordinator instance */
  workflowCoordinator: CheckpointCoordinator;
  /** Workflow checkpoint dependencies */
  workflowDeps: WorkflowCheckpointDependencies;
  /** Agent loop checkpoint coordinator instance */
  agentCoordinator: AgentLoopCheckpointCoordinator;
  /** Agent checkpoint dependencies */
  agentDeps: AgentCheckpointDependencies;
  /** Agent loop runtime config for restoration (required for agent restore) */
  agentRuntimeConfig: AgentLoopRuntimeConfig;
  /** Hierarchy registry for cross-type registration */
  hierarchyRegistry: ExecutionHierarchyRegistry;
  /** File checkpoint manager (optional) */
  fileCheckpointManager?: FileCheckpointManager;
}

export interface RootRestoreResult {
  /** The restored root entity */
  rootEntity: AnyExecutionEntity;
  /** Total number of entities restored */
  totalRestored: number;
  /** Hierarchy validation result */
  validationResult: HierarchyValidationResult;
  /** List of child entity IDs that failed to restore */
  failedChildren: Array<{ childId: ID; childType: ExecutionType; error: string }>;
  /** Restoration timeline */
  restoredEntities: Array<{ entityId: ID; entityType: ExecutionType; checkpointId: string }>;
}

export class ExecutionRestoreCoordinator {
  /**
   * Restore the root execution entity from a checkpoint and recursively restore all children.
   *
   * Restore order:
   * 1. Restore root entity
   * 2. Collect child references from restored snapshot hierarchy
   * 3. Group children by type (WORKFLOW first, then AGENT_LOOP)
   * 4. Recursively restore each child in order
   * 5. Register all entities in hierarchy registry
   * 6. Validate hierarchy integrity
   *
   * @param checkpointId Root checkpoint ID
   * @param rootType Root execution type
   * @param deps Restoration dependencies
   * @returns RootRestoreResult with full restoration details
   */
  async restoreRoot(
    checkpointId: string,
    rootType: ExecutionType,
    deps: RestoreDependencies,
  ): Promise<RootRestoreResult> {
    const restoredEntities: Array<{ entityId: ID; entityType: ExecutionType; checkpointId: string }> = [];
    const failedChildren: Array<{ childId: ID; childType: ExecutionType; error: string }> = [];

    const rootEntity = await this.restoreSingleEntity(checkpointId, rootType, deps, restoredEntities, failedChildren);

    // Recursively restore children in the correct order
    await this.restoreChildrenRecursive(rootEntity, deps, restoredEntities, failedChildren, new Set<string>([rootEntity.id]));

    // Validate hierarchy integrity for the root
    const rootHierarchy = this.getHierarchyMetadata(rootEntity);
    const validationResult = rootHierarchy
      ? HierarchyIntegrityService.validateIntegrity(rootHierarchy, deps.hierarchyRegistry)
      : { valid: true, issues: [] };

    return {
      rootEntity,
      totalRestored: restoredEntities.length,
      validationResult,
      failedChildren,
      restoredEntities,
    };
  }

  /**
   * Recursively restore children of an entity in the correct order.
   * Restore order by type: WORKFLOW first, then AGENT_LOOP.
   * This ensures workflow children are available before agent children
   * that might depend on them.
   */
  private async restoreChildrenRecursive(
    entity: AnyExecutionEntity,
    deps: RestoreDependencies,
    restoredEntities: Array<{ entityId: ID; entityType: ExecutionType; checkpointId: string }>,
    failedChildren: Array<{ childId: ID; childType: ExecutionType; error: string }>,
    visited: Set<string>,
  ): Promise<void> {
    const children = entity.getChildReferences();

    if (children.length === 0) return;

    // Group children by type and restore WORKFLOW before AGENT_LOOP
    const workflowChildren = children.filter(c => c.childType === "WORKFLOW");
    const agentChildren = children.filter(c => c.childType === "AGENT_LOOP");
    const orderedChildren: ChildExecutionReference[] = [...workflowChildren, ...agentChildren];

    for (const childRef of orderedChildren) {
      if (visited.has(childRef.childId)) continue;
      visited.add(childRef.childId);

      const childCheckpointId = await this.findChildCheckpoint(childRef, deps);

      if (!childCheckpointId) {
        failedChildren.push({
          childId: childRef.childId,
          childType: childRef.childType,
          error: "No checkpoint found for child execution",
        });
        continue;
      }

      try {
        const childEntity = await this.restoreSingleEntity(
          childCheckpointId,
          childRef.childType,
          deps,
          restoredEntities,
          failedChildren,
        );

        const parentType = entity.instanceType === "workflowExecution" ? "WORKFLOW" : "AGENT_LOOP";
        childEntity.setParentContext({
          parentType,
          parentId: entity.id,
        });

        entity.registerChild(childRef);

        await this.restoreChildrenRecursive(childEntity, deps, restoredEntities, failedChildren, visited);
      } catch (error) {
        failedChildren.push({
          childId: childRef.childId,
          childType: childRef.childType,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
  }

  /**
   * Restore a single entity from checkpoint and register it in the hierarchy registry.
   */
  private async restoreSingleEntity(
    checkpointId: string,
    entityType: ExecutionType,
    deps: RestoreDependencies,
    restoredEntities: Array<{ entityId: ID; entityType: ExecutionType; checkpointId: string }>,
    _failedChildren: Array<{ childId: ID; childType: ExecutionType; error: string }>,
  ): Promise<AnyExecutionEntity> {
    if (entityType === "WORKFLOW") {
      const result = await deps.workflowCoordinator.restoreWorkflowFromCheckpoint(checkpointId, deps.workflowDeps);

      restoredEntities.push({
        entityId: result.workflowExecutionEntity.id,
        entityType: "WORKFLOW",
        checkpointId,
      });

      if (deps.fileCheckpointManager) {
        await this.restoreFileCheckpoint(result.workflowExecutionEntity.id, deps);
      }

      return result.workflowExecutionEntity;
    }

    const result = await deps.agentCoordinator.restoreAgentLoopFromCheckpoint(
      checkpointId,
      deps.agentDeps,
      deps.agentRuntimeConfig,
    );

    restoredEntities.push({
      entityId: result.id,
      entityType: "AGENT_LOOP",
      checkpointId,
    });

    if (deps.fileCheckpointManager) {
      await this.restoreFileCheckpoint(result.id, deps);
    }

    return result;
  }

  /**
   * Restore file checkpoint for an entity (non-fatal on failure).
   */
  private async restoreFileCheckpoint(entityId: string, deps: RestoreDependencies): Promise<void> {
    if (!deps.fileCheckpointManager) return;

    try {
      const fileCheckpoints = await deps.fileCheckpointManager.getStorage().listByEntity(entityId, { limit: 1 });
      if (fileCheckpoints.length > 0) {
        await deps.fileCheckpointManager.restoreCheckpoint(entityId, fileCheckpoints[0]!.id);
      }
    } catch {
      // Non-fatal: file checkpoint restore failure should not block execution restore
    }
  }

  /**
   * Find the latest checkpoint for a child execution.
   */
  private async findChildCheckpoint(
    childRef: { childId: ID; childType: ExecutionType },
    deps: RestoreDependencies,
  ): Promise<string | undefined> {
    try {
      if (childRef.childType === "WORKFLOW") {
        const checkpointIds = await deps.workflowDeps.checkpointStateManager.list({ parentId: childRef.childId });
        if (checkpointIds.length === 0) return undefined;
        return checkpointIds[checkpointIds.length - 1];
      }

      const checkpointIds = await deps.agentDeps.listCheckpoints(childRef.childId);
      if (checkpointIds.length === 0) return undefined;
      return checkpointIds[checkpointIds.length - 1];
    } catch {
      return undefined;
    }
  }

  /**
   * Get hierarchy metadata from an entity.
   */
  private getHierarchyMetadata(entity: AnyExecutionEntity): ExecutionHierarchyMetadata | undefined {
    if ("getHierarchyMetadata" in entity) {
      return (entity as { getHierarchyMetadata(): ExecutionHierarchyMetadata | undefined }).getHierarchyMetadata();
    }
    return undefined;
  }
}
