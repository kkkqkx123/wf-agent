/**
 * NodeCheckpointStrategy - Encapsulates checkpoint creation logic for node execution
 *
 * Extracted from NodeExecutionCoordinator to reduce its responsibilities and
 * centralize checkpoint creation decision logic.
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { RuntimeNode, WorkflowNode, StaticNode, CheckpointConfig } from "@wf-agent/types";
import { CheckpointTrigger } from "@wf-agent/types";
import { CheckpointCoordinator, type CheckpointDependencies } from "../../checkpoint/checkpoint-coordinator.js";
import {
  resolveCheckpointConfig,
  buildNodeCheckpointLayers,
} from "../../checkpoint/utils/config-resolver.js";
import { handleErrorWithContext } from "../../../shared/utils/error-utils.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { SDKError } from "@wf-agent/types";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "node-checkpoint-strategy" });

/**
 * Resolves a StaticNode from a RuntimeNode or WorkflowNode reference.
 */
function resolveStaticNode(node: RuntimeNode | WorkflowNode): StaticNode | undefined {
  return "originalNode" in node ? (node as WorkflowNode).originalNode : (node as StaticNode | undefined);
}

/**
 * Node checkpont strategy that centralizes all checkpoint creation logic during node execution.
 */
export class NodeCheckpointStrategy {
  constructor(
    private readonly checkpointDependencies: CheckpointDependencies | undefined,
    private readonly globalCheckpointConfig: CheckpointConfig | undefined,
    private readonly eventManager: EventRegistry,
  ) {}

  /**
   * Create a checkpoint before node execution.
   * Returns true if a checkpoint was created.
   */
  async createBeforeNodeCheckpoint(
    workflowExecutionEntity: WorkflowExecutionEntity,
    node: RuntimeNode | WorkflowNode,
    nodeId: string,
  ): Promise<boolean> {
    if (!this.checkpointDependencies) return false;

    const context = { triggerType: CheckpointTrigger.BEFORE_EXECUTE, nodeId };
    const staticNode = resolveStaticNode(node);
    const layers = buildNodeCheckpointLayers(this.globalCheckpointConfig, staticNode, context);
    const configResult = resolveCheckpointConfig(layers, context);

    if (!configResult.shouldCreate) return false;

    logger.debug("Creating checkpoint before node execution", { executionId: workflowExecutionEntity.id, nodeId });
    try {
      const coordinator = new CheckpointCoordinator();
      await coordinator.createWorkflowCheckpoint(workflowExecutionEntity, this.checkpointDependencies, {
        nodeId,
        metadata: {
          description:
            configResult.description ||
            `Before node: ${(node as WorkflowNode).name || (node as RuntimeNode as WorkflowNode).originalNode?.name}`,
        },
      });
      return true;
    } catch (error) {
      await handleErrorWithContext(
        this.eventManager,
        getErrorOrNew(error) as SDKError,
        workflowExecutionEntity,
        "CREATE_CHECKPOINT",
      );
      logger.warn("Before-node checkpoint creation failed, continuing execution", {
        executionId: workflowExecutionEntity.id,
        nodeId,
        error: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }

  /**
   * Create a checkpoint after node execution.
   * Returns true if a checkpoint was created.
   */
  async createAfterNodeCheckpoint(
    workflowExecutionEntity: WorkflowExecutionEntity,
    node: RuntimeNode | WorkflowNode,
    nodeId: string,
  ): Promise<boolean> {
    if (!this.checkpointDependencies) return false;

    const context = { triggerType: CheckpointTrigger.AFTER_EXECUTE, nodeId };
    const staticNode = resolveStaticNode(node);
    const layers = buildNodeCheckpointLayers(this.globalCheckpointConfig, staticNode, context);
    const configResult = resolveCheckpointConfig(layers, context);

    if (!configResult.shouldCreate) return false;

    logger.debug("Creating checkpoint after node execution", { executionId: workflowExecutionEntity.id, nodeId });
    try {
      const coordinator = new CheckpointCoordinator();
      await coordinator.createWorkflowCheckpoint(workflowExecutionEntity, this.checkpointDependencies, {
        nodeId,
        metadata: {
          description:
            configResult.description ||
            `After node: ${(node as WorkflowNode).name || (node as RuntimeNode as WorkflowNode).originalNode?.name}`,
        },
      });
      return true;
    } catch (error) {
      await handleErrorWithContext(
        this.eventManager,
        getErrorOrNew(error) as SDKError,
        workflowExecutionEntity,
        "CREATE_CHECKPOINT_AFTER",
      );
      logger.warn("After-node checkpoint creation failed, continuing execution", {
        executionId: workflowExecutionEntity.id,
        nodeId,
        error: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }

  /**
   * Create a checkpoint on node failure after retries are exhausted.
   * Non-fatal: checkpoint creation failure does not mask the original node failure.
   */
  async createFailureCheckpoint(
    workflowExecutionEntity: WorkflowExecutionEntity,
    nodeId: string,
    nodeType: string,
    retryCount: number,
  ): Promise<void> {
    if (!this.checkpointDependencies) return;

    try {
      const coordinator = new CheckpointCoordinator();
      await coordinator.createWorkflowCheckpoint(workflowExecutionEntity, this.checkpointDependencies, {
        nodeId,
        metadata: {
          description: `Node failed: ${nodeId} (${nodeType})`,
          customFields: {
            failureType: "node_execution_failure",
            failedAt: Date.now(),
            retryCount,
          },
        },
      });
      logger.info("Failure checkpoint created for node", { executionId: workflowExecutionEntity.id, nodeId, retryCount });
    } catch (cpError) {
      logger.warn("Failed to create failure checkpoint, continuing", {
        executionId: workflowExecutionEntity.id,
        nodeId,
        error: cpError,
      });
    }
  }

  /**
   * Create a checkpoint when an exception is caught during node execution.
   * Non-fatal: checkpoint creation failure does not mask the original error.
   */
  async createExceptionCheckpoint(
    workflowExecutionEntity: WorkflowExecutionEntity,
    nodeId: string,
    nodeType: string,
  ): Promise<void> {
    if (!this.checkpointDependencies) return;

    try {
      const coordinator = new CheckpointCoordinator();
      await coordinator.createWorkflowCheckpoint(workflowExecutionEntity, this.checkpointDependencies, {
        nodeId,
        metadata: {
          description: `Node execution threw: ${nodeId} (${nodeType})`,
          customFields: {
            failureType: "node_execution_exception",
            failedAt: Date.now(),
          },
        },
      });
      logger.info("Failure checkpoint created in catch block", { executionId: workflowExecutionEntity.id, nodeId });
    } catch (cpError) {
      logger.warn("Failed to create failure checkpoint in catch block, continuing", {
        executionId: workflowExecutionEntity.id,
        nodeId,
        error: cpError,
      });
    }
  }
}