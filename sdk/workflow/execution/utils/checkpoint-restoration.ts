/**
 * Checkpoint Restoration Utilities
 * Handles restoring workflow state from checkpoints
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { CheckpointDependencies } from "../../checkpoint/utils/checkpoint-utils.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "checkpoint-restoration" });

export interface RestorationResult {
  success: boolean;
  checkpointId?: string;
  restoredNodeId?: string;
  error?: Error;
}

/**
 * Get the latest checkpoint for a workflow execution
 */
async function getLatestCheckpoint(
  executionId: string,
  dependencies: CheckpointDependencies,
): Promise<any | null> {
  try {
    const { checkpointStateManager } = dependencies;

    // List checkpoints for this execution, sorted by timestamp descending
    const checkpointIds = await (checkpointStateManager as any).storageAdapter.list({
      executionId,
      sortBy: "timestamp",
      sortOrder: "desc",
      limit: 1,
    });

    if (checkpointIds.length === 0) {
      logger.debug("No checkpoints found for execution", { executionId });
      return null;
    }

    // Load the latest checkpoint
    const latestCheckpointId = checkpointIds[0];
    const data = await (checkpointStateManager as any).storageAdapter.load(latestCheckpointId);

    if (!data) {
      logger.warn("Failed to load latest checkpoint data", { checkpointId: latestCheckpointId });
      return null;
    }

    // Deserialize the checkpoint
    const checkpoint = await (checkpointStateManager as any).checkpointSerializer.deserializeCheckpoint(data);

    logger.info("Found latest checkpoint", {
      executionId,
      checkpointId: latestCheckpointId,
      nodeId: checkpoint.nodeId || checkpoint.metadata?.customFields?.nodeId,
      timestamp: checkpoint.timestamp,
    });

    return checkpoint;
  } catch (error) {
    logger.error("Failed to get latest checkpoint", {
      executionId,
      error: error instanceof Error ? error.message : String(error),
    });
    return null;
  }
}

/**
 * Restore workflow execution from latest checkpoint
 */
export async function restoreWorkflowFromCheckpoint(
  executionId: string,
  workflowExecutionEntity: WorkflowExecutionEntity,
  registry: WorkflowExecutionRegistry,
): Promise<RestorationResult> {
  try {
    // Get checkpoint dependencies from container
    const container = await import("../../../core/di/index.js").then(m => m.getContainer());
    const Identifiers = await import("../../../core/di/service-identifiers.js");
    const checkpointDeps = {
      workflowExecutionRegistry: registry,
      checkpointStateManager: container.get(Identifiers.CheckpointState),
      workflowRegistry: container.get(Identifiers.WorkflowRegistry),
      workflowGraphRegistry: container.get(Identifiers.WorkflowGraphRegistry),
    } as CheckpointDependencies;

    // Get latest checkpoint for this execution
    const checkpoint = await getLatestCheckpoint(executionId, checkpointDeps);

    if (!checkpoint) {
      logger.debug("No checkpoint found for execution", { executionId });
      return { success: false };
    }

    logger.info("Restoring from checkpoint", {
      executionId,
      checkpointId: checkpoint.id,
      nodeId: checkpoint.nodeId || checkpoint.metadata?.customFields?.nodeId,
    });

    // Restore node position
    const nodeId = checkpoint.nodeId || checkpoint.metadata?.customFields?.nodeId;
    if (nodeId) {
      workflowExecutionEntity.setCurrentNodeId(nodeId);
    }

    // Restore variable state if available
    if (checkpoint.metadata?.customFields?.variables) {
      const variables = checkpoint.metadata.customFields.variables as Record<string, unknown>;
      // Convert object to array format expected by restoreFromSnapshot
      const variableArray = Object.entries(variables).map(([name, value]) => ({
        name,
        value,
        type: "any" as any,
        scope: "WORKFLOW" as any,
        readonly: false,
      }));
      workflowExecutionEntity.variableStateManager.restoreFromSnapshot({
        variables: variableArray,
        variableScopes: {
          global: {},
          workflowExecution: {},
          local: [],
          loop: [],
        },
      });
      logger.debug("Variable state restored from checkpoint");
    }

    // Restore message history if available
    if (checkpoint.metadata?.customFields?.messages) {
      const messages = checkpoint.metadata.customFields.messages as any[];
      workflowExecutionEntity.messageHistoryManager.restoreFromSnapshot({ messages });
      logger.debug("Message history restored from checkpoint");
    }

    return {
      success: true,
      checkpointId: checkpoint.id,
      restoredNodeId: nodeId,
    };
  } catch (error) {
    logger.error("Failed to restore from checkpoint", {
      executionId,
      error: error instanceof Error ? error.message : String(error),
    });

    return {
      success: false,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
}
