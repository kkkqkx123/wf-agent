/**
 * Agent Loop checkpoint tool function
 *
 * Provides a functional checkpoint creation interface
 */

import type { ID } from "@wf-agent/types";
import type { CheckpointMetadata } from "@wf-agent/types";
import type { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import {
  AgentLoopCheckpointCoordinator,
  type CheckpointDependencies,
} from "./checkpoint-coordinator.js";
import { mergeMetadata } from "../../utils/metadata-utils.js";
import type { Metadata } from "@wf-agent/types";

/**
 * Checkpoint creation options
 */
export interface CreateCheckpointOptions {
  /** Agent Loop ID */
  agentLoopId?: ID;
  /** Checkpoint Description */
  description?: string;
  /** Customized metadata */
  metadata?: CheckpointMetadata;
}

/**
 * Creating Checkpoints (Functional Interfaces)
 * @param entity Agent Loop entity
 * @param dependencies Checkpoint dependencies
 * @param options Checkpoint creation options
 * @returns Checkpoint ID
 */
export async function createCheckpoint(
  entity: AgentLoopEntity,
  dependencies: CheckpointDependencies,
  options: CreateCheckpointOptions = {},
): Promise<string> {
  const { description, metadata } = options;

  // Constructing checkpoint metadata
  const checkpointMetadata: CheckpointMetadata = mergeMetadata((metadata as Metadata) || {}, {
    description: description || `Checkpoint for agent loop ${entity.id}`,
  });

  // Creating coordinator instance and create checkpoint
  const coordinator = new AgentLoopCheckpointCoordinator();
  return await coordinator.createCheckpoint(entity, dependencies, {
    metadata: checkpointMetadata,
  });
}

/**
 * Recover Agent Loop Entity from Checkpoint (Functional Interface)
 * @param checkpointId Checkpoint ID
 * @param dependencies checkpoint dependencies
 * @returns Agent Loop instance
 */
export async function restoreFromCheckpoint(
  checkpointId: string,
  dependencies: CheckpointDependencies,
): Promise<AgentLoopEntity> {
  const coordinator = new AgentLoopCheckpointCoordinator();
  return await coordinator.restoreFromCheckpoint(checkpointId, dependencies);
}
