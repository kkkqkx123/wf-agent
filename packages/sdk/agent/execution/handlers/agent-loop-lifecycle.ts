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
import { AgentLoopCheckpointCoordinator, type CheckpointDependencies } from "../../checkpoint/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { CheckpointMetadata } from "@wf-agent/types";
import { AgentStateCoordinator } from "../../state-managers/agent-state-coordinator.js";
import { ConversationSession } from "../../../shared/messaging/conversation-session.js";

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
  description?: string;
  tags?: string[];
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
    dependencies as CheckpointDependencies,
    {
      metadata: options?.metadata as CheckpointMetadata | undefined,
      description: options?.description,
      tags: options?.tags,
    },
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
 * Delegates to entity.cleanup() for unified resource management.
 * @param entity Agent Loop instance
 */
export function cleanupAgentLoop(entity: AgentLoopEntity): void {
  logger.debug("Cleaning up Agent Loop resources", {
    agentLoopId: entity.id,
    iteration: entity.state.currentIteration,
    status: entity.getStatus(),
  });

  entity.cleanup();

  logger.info("Agent Loop resources cleaned up", {
    agentLoopId: entity.id,
    iteration: entity.state.currentIteration,
  });
}

/**
 * Cloning the Agent Loop Entity
 * @param entity Agent Loop entity
 * @param stateCoordinator Agent State Coordinator
 * @returns Cloned entity and new state coordinator
 */
export function cloneAgentLoop(
  entity: AgentLoopEntity,
  stateCoordinator: AgentStateCoordinator,
): { entity: AgentLoopEntity; stateCoordinator: AgentStateCoordinator } {
  logger.debug("Cloning Agent Loop entity", {
    agentLoopId: entity.id,
    iteration: entity.state.currentIteration,
    status: entity.getStatus(),
  });

  const cloned = new AgentLoopEntity(entity.id, { ...entity.config }, entity.state.clone());

  // Clone message history using state coordinator snapshot/restore
  const messageSnapshot = stateCoordinator.createSnapshot();
  const newConversationSession = new ConversationSession({
    executionId: cloned.id,
  });
  const newStateCoordinator = new AgentStateCoordinator({
    conversationManager: newConversationSession,
  });
  newStateCoordinator.restoreFromSnapshot(messageSnapshot);

  // Clone parent context using unified hierarchy API
  const parentContext = entity.getParentContext();
  if (parentContext) {
    cloned.setParentContext(parentContext);
  }

  logger.info("Agent Loop entity cloned successfully", {
    agentLoopId: entity.id,
    clonedAgentLoopId: cloned.id,
    iteration: entity.state.currentIteration,
  });

  return { entity: cloned, stateCoordinator: newStateCoordinator };
}
