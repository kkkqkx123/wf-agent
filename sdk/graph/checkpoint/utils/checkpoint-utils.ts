/**
 * Checkpoint tool function
 * Provides a functional interface for creating checkpoints
 */

import type { ID } from "@wf-agent/types";
import type { CheckpointMetadata } from "@wf-agent/types";
import type { ThreadRegistry } from "../../stores/thread-registry.js";
import type { WorkflowRegistry } from "../../stores/workflow-registry.js";
import type { GraphRegistry } from "../../stores/graph-registry.js";
import { CheckpointState } from "../checkpoint-state-manager.js";
import { CheckpointCoordinator } from "../checkpoint-coordinator.js";
import { mergeMetadata } from "../../../utils/metadata-utils.js";
import type { Metadata } from "@wf-agent/types";

/**
 * Checkpoint creation options
 */
export interface CreateCheckpointOptions {
  /** Thread ID */
  threadId: ID;
  /** Node ID (optional) */
  nodeId?: ID;
  /** Tool ID (optional) */
  toolId?: ID;
  /** Checkpoint Description */
  description?: string;
  /** Custom metadata */
  metadata?: CheckpointMetadata;
}

/**
 * Checkpoint dependencies
 */
export interface CheckpointDependencies {
  /** Thread Registry */
  threadRegistry: ThreadRegistry;
  /** Checkpoint State Manager */
  checkpointStateManager: CheckpointState;
  /** Workflow Registry */
  workflowRegistry: WorkflowRegistry;
  /** Figure Registry */
  graphRegistry: GraphRegistry;
}

/**
 * Create a checkpoint (functional interface)
 * @param options Checkpoint creation options
 * @param dependencies Checkpoint dependencies
 * @returns Checkpoint ID
 */
export async function createCheckpoint(
  options: CreateCheckpointOptions,
  dependencies: CheckpointDependencies,
): Promise<string> {
  const { threadId, nodeId, toolId, description, metadata } = options;

  // Build checkpoint metadata
  const checkpointMetadata: CheckpointMetadata = mergeMetadata((metadata as Metadata) || {}, {
    description:
      description ||
      `Checkpoint${nodeId ? ` for node ${nodeId}` : toolId ? ` for tool ${toolId}` : ""}`,
    customFields: mergeMetadata((metadata?.customFields || {}) as Metadata, { nodeId, toolId }),
  });

  // Call a static method to create a checkpoint.
  return await CheckpointCoordinator.createCheckpoint(threadId, dependencies, checkpointMetadata);
}

/**
 * Create checkpoints in batches
 * @param optionsList List of checkpoint creation options
 * @param dependencies List of checkpoint dependencies
 * @returns Array of checkpoint IDs
 */
export async function createCheckpoints(
  optionsList: CreateCheckpointOptions[],
  dependencies: CheckpointDependencies,
): Promise<string[]> {
  const results: string[] = [];

  // Create checkpoints in parallel.
  const promises = optionsList.map(options => createCheckpoint(options, dependencies));

  // Wait for all checkpoints to be created.
  const checkpointIds = await Promise.all(promises);
  results.push(...checkpointIds);

  return results;
}

/**
 * Create a node-level checkpoint (convenient function)
 * @param threadId Thread ID
 * @param nodeId Node ID
 * @param description Checkpoint description
 * @param dependencies Checkpoint dependencies
 * @returns Checkpoint ID
 */
export async function createNodeCheckpoint(
  threadId: ID,
  nodeId: ID,
  dependencies: CheckpointDependencies,
  description?: string,
): Promise<string> {
  return createCheckpoint(
    {
      threadId,
      nodeId,
      description: description || `Node checkpoint for node ${nodeId}`,
    },
    dependencies,
  );
}

/**
 * Create a tool-level checkpoint (convenience function)
 * @param threadId Thread ID
 * @param toolName Tool name
 * @param description Checkpoint description
 * @param dependencies Checkpoint dependencies
 * @returns Checkpoint ID
 */
export async function createToolCheckpoint(
  threadId: ID,
  toolId: ID,
  dependencies: CheckpointDependencies,
  description?: string,
): Promise<string> {
  return createCheckpoint(
    {
      threadId,
      toolId,
      description: description || `Tool checkpoint for tool ${toolId}`,
    },
    dependencies,
  );
}
