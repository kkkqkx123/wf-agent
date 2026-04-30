/**
 * Agent Loop Life Cycle Processing Functions
 *
 * Responsible for the lifecycle management of the AgentLoopEntity, including:
 * - Checkpoint creation
 * - Resource cleanup
 * - Instance cloning
 *
 * Design Principles:
 * - Functional export: use pure functions rather than static class methods
 * - Decoupling from entity classes: separation of life cycle logic from entity classes
 * - Support for state persistence: support for checkpoints
 */

import { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import { AgentLoopCheckpointCoordinator } from "../../checkpoint/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentLoopLifecycle" });

/**
 * checkpoint dependency
 */
export interface AgentLoopCheckpointDependencies {
  saveCheckpoint: (checkpoint: unknown) => Promise<string>;
  getCheckpoint: (id: string) => Promise<unknown>;
  listCheckpoints: (agentLoopId: string) => Promise<string[]>;
  deltaConfig?: unknown;
}

/**
 * Checkpoint creation options
 */
export interface AgentLoopCheckpointOptions {
  metadata?: unknown;
}

/**
 * Creating an Agent Loop Checkpoint
 * @param entity Agent Loop entity
 * @param dependencies Checkpoint dependencies
 * @param options Checkpoint creation options
 * @returns Checkpoint ID
 */
export async function createAgentLoopCheckpoint(
  entity: AgentLoopEntity,
  dependencies: AgentLoopCheckpointDependencies,
  options?: AgentLoopCheckpointOptions,
): Promise<string> {
  logger.info("Creating Agent Loop checkpoint", {
    agentLoopId: entity.id,
    iteration: entity.state.currentIteration,
    status: entity.getStatus(),
  });

  const coordinator = new AgentLoopCheckpointCoordinator();
  const checkpointId = await coordinator.createCheckpoint(
    entity,
    dependencies as import("../../checkpoint/checkpoint-coordinator.js").CheckpointDependencies,
    options
      ? { metadata: options.metadata as import("@wf-agent/types").CheckpointMetadata }
      : undefined,
  );

  logger.info("Agent Loop checkpoint created successfully", {
    agentLoopId: entity.id,
    checkpointId,
    iteration: entity.state.currentIteration,
  });

  return checkpointId;
}

/**
 * Clean up Agent Loop resources
 * @param entity Agent Loop instance
 */
export function cleanupAgentLoop(entity: AgentLoopEntity): void {
  logger.debug("Cleaning up Agent Loop resources", {
    agentLoopId: entity.id,
    iteration: entity.state.currentIteration,
    status: entity.getStatus(),
  });

  entity.state.cleanup();
  entity.conversationManager.cleanup();
  entity.variableStateManager.cleanup();
  entity.abortController = undefined;

  logger.info("Agent Loop resources cleaned up", {
    agentLoopId: entity.id,
    iteration: entity.state.currentIteration,
  });
}

/**
 * Cloning the Agent Loop Entity
 * @param entity Agent Loop entity
 * @returns Cloned entity
 */
export function cloneAgentLoop(entity: AgentLoopEntity): AgentLoopEntity {
  logger.debug("Cloning Agent Loop entity", {
    agentLoopId: entity.id,
    iteration: entity.state.currentIteration,
    status: entity.getStatus(),
  });

  const cloned = new AgentLoopEntity(entity.id, { ...entity.config }, entity.state.clone());

  // Cloning News History
  const messageSnapshot = entity.conversationManager.createSnapshot();
  cloned.conversationManager.restoreFromSnapshot(messageSnapshot);

  // clone variable state
  const variableSnapshot = entity.variableStateManager.createSnapshot();
  cloned.variableStateManager.restoreFromSnapshot(variableSnapshot);

  cloned.parentExecutionId = entity.parentExecutionId;
  cloned.nodeId = entity.nodeId;

  logger.info("Agent Loop entity cloned successfully", {
    agentLoopId: entity.id,
    clonedAgentLoopId: cloned.id,
    iteration: entity.state.currentIteration,
  });

  return cloned;
}
