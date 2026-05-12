/**
 * Checkpoint Restoration Utilities
 * Handles restoring workflow state from checkpoints
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { CheckpointDependencies } from "../../checkpoint/utils/checkpoint-utils.js";
import type { Checkpoint, LLMMessage } from "@wf-agent/types";
import type { GlobalContext } from "../../../core/global-context.js";
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
): Promise<Checkpoint | null> {
  try {
    const { checkpointStateManager } = dependencies;

    // List checkpoints for this execution, sorted by timestamp descending
    const checkpointIds = await checkpointStateManager.list({
      parentId: executionId,
      limit: 1,
    });

    if (checkpointIds.length === 0) {
      logger.debug("No checkpoints found for execution", { executionId });
      return null;
    }

    // Load the latest checkpoint
    const latestCheckpointId = checkpointIds[0];
    if (!latestCheckpointId) {
      logger.debug("No checkpoint ID found", { executionId });
      return null;
    }
    
    const checkpoint = await checkpointStateManager.get(latestCheckpointId);

    if (!checkpoint) {
      logger.warn("Failed to load latest checkpoint", { checkpointId: latestCheckpointId });
      return null;
    }

    logger.info("Restoring from checkpoint", {
      executionId,
      checkpointId: checkpoint.id,
      nodeId: checkpoint.metadata?.customFields?.["nodeId"] as string | undefined,
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
  globalContext: GlobalContext,
): Promise<RestorationResult> {
  try {
    // Get checkpoint dependencies from container
    const Identifiers = await import("../../../core/di/service-identifiers.js");
    const container = globalContext.container;
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
      nodeId: checkpoint.metadata?.customFields?.["nodeId"] as string | undefined,
    });

    // Restore node position
    const nodeId = checkpoint.metadata?.customFields?.["nodeId"] as string | undefined;
    if (nodeId) {
      workflowExecutionEntity.setCurrentNodeId(nodeId);
    }

    // Restore complete workflow state from checkpoint snapshot
    // Note: This uses the same logic as CheckpointCoordinator.restoreFromCheckpoint()
    const checkpointData = checkpoint as import("@wf-agent/types").FullCheckpoint<any>;
    const workflowExecutionState = checkpointData.snapshot;
    
    if (workflowExecutionState && workflowExecutionState.variableScopes) {
      const globalMap = new Map();
      const executionMap = new Map();
      const scopeStack: Map<string, any>[] = [];
      
      const scopes = workflowExecutionState.variableScopes;
      
      // Restore global variables
      if (scopes.global) {
        for (const [name, value] of Object.entries(scopes.global)) {
          globalMap.set(name, {
            definition: { name, type: typeof value, value, scope: 'global' as const },
            value,
          });
        }
      }
      
      // Restore execution variables
      if (scopes.execution) {
        for (const [name, value] of Object.entries(scopes.execution)) {
          executionMap.set(name, {
            definition: { name, type: typeof value, value, scope: 'execution' as const },
            value,
          });
        }
      }
      
      // Restore subgraph scopes (scopeStack)
      if (scopes.subgraph && Array.isArray(scopes.subgraph)) {
        for (const scopeObj of scopes.subgraph) {
          const scopeMap = new Map();
          for (const [name, value] of Object.entries(scopeObj)) {
            scopeMap.set(name, {
              definition: { name, type: typeof value, value, scope: 'subgraph' as const },
              value,
            });
          }
          scopeStack.push(scopeMap);
        }
      }
      
      workflowExecutionEntity.variableStateManager.restoreFromSnapshot({
        global: globalMap,
        execution: executionMap,
        scopeStack,
      });
      logger.debug("Variable state restored from checkpoint snapshot");
    }

    // Restore message history if available
    if (checkpoint.metadata?.customFields?.["messages"]) {
      const messages = checkpoint.metadata.customFields["messages"] as LLMMessage[];
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
