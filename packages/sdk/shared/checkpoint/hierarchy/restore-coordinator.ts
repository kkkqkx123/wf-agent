import type { ExecutionType, ExecutionHierarchyMetadata, ChildExecutionReference } from "@wf-agent/types";
import type { ID, AgentLoopRuntimeConfig } from "@wf-agent/types";
import type { AnyExecutionEntity, ExecutionHierarchyRegistry } from "../../registry/execution-hierarchy-registry.js";
import type { HierarchyValidationResult } from "../../execution/hierarchy-integrity-service.js";
import { HierarchyIntegrityService } from "../../execution/hierarchy-integrity-service.js";
import type { CheckpointCoordinator, WorkflowCheckpointDependencies } from "../../../workflow/checkpoint/checkpoint-coordinator.js";
import type { AgentLoopCheckpointCoordinator, CheckpointDependencies as AgentCheckpointDependencies } from "../../../agent/checkpoint/checkpoint-coordinator.js";
import type { FileCheckpointManager } from "@wf-agent/common-utils";

export interface RestoreDependencies {
  workflowCoordinator: CheckpointCoordinator;
  workflowDeps: WorkflowCheckpointDependencies;
  agentCoordinator: AgentLoopCheckpointCoordinator;
  agentDeps: AgentCheckpointDependencies;
  agentRuntimeConfig: AgentLoopRuntimeConfig;
  hierarchyRegistry: ExecutionHierarchyRegistry;
  fileCheckpointManager?: FileCheckpointManager;
}

export interface RootRestoreResult {
  rootEntity: AnyExecutionEntity;
  totalRestored: number;
  validationResult: HierarchyValidationResult;
  failedChildren: Array<{ childId: ID; childType: ExecutionType; error: string }>;
  restoredEntities: Array<{ entityId: ID; entityType: ExecutionType; checkpointId: string }>;
}

export class ExecutionRestoreCoordinator {
  async restoreRoot(
    checkpointId: string,
    rootType: ExecutionType,
    deps: RestoreDependencies,
  ): Promise<RootRestoreResult> {
    const restoredEntities: Array<{ entityId: ID; entityType: ExecutionType; checkpointId: string }> = [];
    const failedChildren: Array<{ childId: ID; childType: ExecutionType; error: string }> = [];

    const rootEntity = await this.restoreSingleEntity(checkpointId, rootType, deps, restoredEntities, failedChildren);

    await this.restoreChildrenRecursive(rootEntity, deps, restoredEntities, failedChildren, new Set<string>([rootEntity.id]));

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

  private async restoreChildrenRecursive(
    entity: AnyExecutionEntity,
    deps: RestoreDependencies,
    restoredEntities: Array<{ entityId: ID; entityType: ExecutionType; checkpointId: string }>,
    failedChildren: Array<{ childId: ID; childType: ExecutionType; error: string }>,
    visited: Set<string>,
  ): Promise<void> {
    const children = entity.getChildReferences();

    if (children.length === 0) return;

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

  private async restoreFileCheckpoint(entityId: string, deps: RestoreDependencies): Promise<void> {
    if (!deps.fileCheckpointManager) return;

    try {
      const fileCheckpoints = await deps.fileCheckpointManager.getStorage().listByEntity(entityId, { limit: 1 });
      if (fileCheckpoints.length > 0) {
        await deps.fileCheckpointManager.restoreCheckpoint(entityId, fileCheckpoints[0]!.id);
      }
    } catch {
    }
  }

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

  private getHierarchyMetadata(entity: AnyExecutionEntity): ExecutionHierarchyMetadata | undefined {
    if ("getHierarchyMetadata" in entity) {
      return (entity as { getHierarchyMetadata(): ExecutionHierarchyMetadata | undefined }).getHierarchyMetadata();
    }
    return undefined;
  }
}
