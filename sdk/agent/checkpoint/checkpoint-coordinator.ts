/**
 * Agent Loop Checkpoint Coordinator
 *
 * A service that coordinates the entire checkpoint process
 * Supports dependency injection for better testability
 */

import { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import { AgentLoopState } from "../entities/agent-loop-state.js";
import type {
  CheckpointMetadata,
  DeltaStorageConfig,
  TCheckpointType,
  AgentLoopCheckpoint,
  AgentLoopStateSnapshot,
} from "@wf-agent/types";
import { CheckpointType, AgentCheckpointError } from "@wf-agent/types";
import { AgentLoopDiffCalculator } from "./agent-loop-diff-calculator.js";
import { AgentLoopDeltaRestorer } from "./agent-loop-delta-restorer.js";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../../core/utils/checkpoint/constants.js";
import { generateId } from "../../utils/index.js";
import { now } from "@wf-agent/common-utils";

/**
 * Checkpoint creation options
 */
export interface CheckpointOptions {
  /** Checkpoint metadata */
  metadata?: CheckpointMetadata;
}

/**
 * Checkpoint dependencies
 */
export interface CheckpointDependencies {
  /** Save checkpoints */
  saveCheckpoint: (checkpoint: AgentLoopCheckpoint) => Promise<string>;
  /** Get checkpoints */
  getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>;
  /** List the checkpoints */
  listCheckpoints: (agentLoopId: string) => Promise<string[]>;
  /** Incremental storage configuration (optional) */
  deltaConfig?: DeltaStorageConfig;
}

/**
 * Agent Loop checkpoint coordinator
 *
 * Design Principles:
 * - Instance-based for dependency injection and testability
 * - Stateless operations - no internal state maintained between calls
 * - Coordinates the entire checkpoint lifecycle
 */
export class AgentLoopCheckpointCoordinator {
  private diffCalculator: AgentLoopDiffCalculator;

  constructor(diffCalculator?: AgentLoopDiffCalculator) {
    this.diffCalculator = diffCalculator ?? new AgentLoopDiffCalculator();
  }

  /**
   * Create a checkpoint
   * @param entity Agent Loop entity
   * @param dependencies dependencies
   * @param options creation options
   * @returns checkpoint ID
   */
  async createCheckpoint(
    entity: AgentLoopEntity,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
  ): Promise<string> {
    const { saveCheckpoint, getCheckpoint, listCheckpoints, deltaConfig } = dependencies;
    const config = { ...DEFAULT_DELTA_STORAGE_CONFIG, ...deltaConfig };

    // Step 1: Extract the current state
    const currentState = this.extractState(entity);

    // Step 2: Retrieve the previous checkpoint
    const previousCheckpointIds = await listCheckpoints(entity.id);
    const checkpointCount = previousCheckpointIds.length;

    // Step 3: Decide on checkpoint type
    const checkpointType = this.determineCheckpointType(checkpointCount, config);

    // Step 4: Generate a unique checkpointId and timestamp
    const checkpointId = generateId();
    const timestamp = now();

    // Step 5: Create Checkpoints
    const checkpoint = await this.buildCheckpoint(
      entity,
      currentState,
      checkpointType,
      checkpointId,
      timestamp,
      checkpointCount,
      previousCheckpointIds,
      getCheckpoint,
      options?.metadata,
    );

    // Step 6: Save the checkpoint
    return await saveCheckpoint(checkpoint);
  }

  /**
   * Restore Agent Loop entity from checkpoint
   * @param checkpointId checkpointId
   * @param dependencies dependencies
   * @returns Recovered Agent Loop Entity
   */
  async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies,
  ): Promise<AgentLoopEntity> {
    const { getCheckpoint, listCheckpoints } = dependencies;

    // Step 1: Load the checkpoint
    const checkpoint = await getCheckpoint(checkpointId);
    if (!checkpoint) {
      throw new AgentCheckpointError(
        `Checkpoint not found: ${checkpointId}`,
        "restore",
        checkpointId,
      );
    }

    // Step 2: Verify checkpoint integrity
    this.validateCheckpoint(checkpoint);

    // Step 3: Obtain the complete status (processing incremental checkpoints)
    const restorer = new AgentLoopDeltaRestorer(getCheckpoint, listCheckpoints);
    const restoreResult = await restorer.restore(checkpointId);
    const { snapshot } = restoreResult;

    // Step 4: Create AgentLoopEntity from snapshot using factory method
    return AgentLoopEntity.fromSnapshot(checkpoint.agentLoopId, snapshot);
  }

  /**
   * Build checkpoint object
   */
  private async buildCheckpoint(
    entity: AgentLoopEntity,
    currentState: AgentLoopStateSnapshot,
    checkpointType: TCheckpointType,
    checkpointId: string,
    timestamp: number,
    checkpointCount: number,
    previousCheckpointIds: string[],
    getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>,
    metadata?: CheckpointMetadata,
  ): Promise<AgentLoopCheckpoint> {
    if (checkpointType === "FULL") {
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL",
        snapshot: currentState,
        metadata,
      };
    }

    // Creating incremental checkpoints
    return this.buildDeltaCheckpoint(
      entity,
      currentState,
      checkpointId,
      timestamp,
      previousCheckpointIds,
      getCheckpoint,
      metadata,
    );
  }

  /**
   * Build delta checkpoint
   */
  private async buildDeltaCheckpoint(
    entity: AgentLoopEntity,
    currentState: AgentLoopStateSnapshot,
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>,
    metadata?: CheckpointMetadata,
  ): Promise<AgentLoopCheckpoint> {
    const previousCheckpointId = previousCheckpointIds[0]!;
    const previousCheckpoint = await getCheckpoint(previousCheckpointId);

    if (!previousCheckpoint) {
      // If the previous checkpoint cannot be obtained, downgrade to the full checkpoint
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL" as const,
        snapshot: currentState,
        metadata,
      };
    }

    // Find baseline checkpoints (full checkpoints with snapshot)
    const baseCheckpoint = await this.findBaseCheckpoint(previousCheckpoint, getCheckpoint);

    // If a checkpoint containing a snapshot is still not found, downgrade to the full checkpoint
    if (!baseCheckpoint?.snapshot) {
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL" as const,
        snapshot: currentState,
        metadata,
      };
    }

    // Calculate the difference with context for optimization
    const delta = this.diffCalculator.calculateDelta(baseCheckpoint.snapshot, currentState, {
      previousMessageCount: baseCheckpoint.snapshot.messages.length,
      currentMessages: entity.getMessages(),
    });

    // Find the baseline checkpoint ID
    const baseCheckpointId =
      previousCheckpoint.type === "FULL"
        ? previousCheckpoint.id
        : previousCheckpoint.baseCheckpointId!;

    return {
      id: checkpointId,
      agentLoopId: entity.id,
      timestamp,
      type: "DELTA",
      baseCheckpointId,
      previousCheckpointId,
      delta,
      metadata,
    };
  }

  /**
   * Find base checkpoint for delta calculation
   */
  private async findBaseCheckpoint(
    previousCheckpoint: AgentLoopCheckpoint,
    getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>,
  ): Promise<AgentLoopCheckpoint | null> {
    if (previousCheckpoint.type === "FULL") {
      return previousCheckpoint;
    }

    // If the previous checkpoint is delta, the nearest complete checkpoint needs to be found
    if (previousCheckpoint.baseCheckpointId) {
      const base = await getCheckpoint(previousCheckpoint.baseCheckpointId);
      if (base && base.snapshot) {
        return base;
      }
    }

    return null;
  }

  /**
   * Extracting a state snapshot
   * @param entity Agent Loop entity
   * @returns Status Snapshot
   */
  private extractState(entity: AgentLoopEntity): AgentLoopStateSnapshot {
    return {
      status: entity.state.status,
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      startTime: entity.state.startTime,
      endTime: entity.state.endTime,
      error: entity.state.error,
      messages: entity.getMessages(),
      variables: entity.getAllVariables(),
      config: entity.config,
    };
  }

  /**
   * Determine the checkpoint type
   * @param checkpointCount: The current number of checkpoints
   * @param config: Incremental storage configuration
   * @returns: The type of checkpoint
   */
  private determineCheckpointType(
    checkpointCount: number,
    config: DeltaStorageConfig,
  ): TCheckpointType {
    // Always create full checkpoints if incremental storage is not enabled
    if (!config.enabled) {
      return "FULL";
    }

    // The first checkpoint must be a complete checkpoint
    if (checkpointCount === 0) {
      return "FULL";
    }

    // Creates a full checkpoint every baselineInterval checkpoints
    if ((checkpointCount + 1) % config.baselineInterval === 0) {
      return "FULL";
    }

    // Create incremental checkpoints in other cases
    return "DELTA";
  }

  /**
   * Verify checkpoint integrity and compatibility
   */
  private validateCheckpoint(checkpoint: AgentLoopCheckpoint): void {
    // Verify required fields
    if (!checkpoint.id || !checkpoint.agentLoopId) {
      throw new AgentCheckpointError(
        "Invalid checkpoint: missing required fields",
        "validate",
        checkpoint.id,
        checkpoint.agentLoopId,
      );
    }

    // Validation against checkpoint type
    if (checkpoint.type === "DELTA") {
      // The incremental checkpoint requires verification of the delta field
      const deltaCheckpoint = checkpoint as import("@wf-agent/types").DeltaCheckpoint<import("@wf-agent/types").AgentLoopDelta>;
      if (!deltaCheckpoint.delta && !deltaCheckpoint.previousCheckpointId) {
        throw new AgentCheckpointError(
          "Invalid delta checkpoint: missing delta data and previous checkpoint reference",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }
    } else {
      // The full checkpoint requires validation of the snapshot field
      const fullCheckpoint = checkpoint as import("@wf-agent/types").FullCheckpoint<import("@wf-agent/types").AgentLoopStateSnapshot>;
      if (!fullCheckpoint.snapshot) {
        throw new AgentCheckpointError(
          "Invalid full checkpoint: missing state snapshot",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }

      // Verify the snapshot structure
      const { snapshot } = fullCheckpoint;
      if (!snapshot.status) {
        throw new AgentCheckpointError(
          "Invalid checkpoint: incomplete state snapshot",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }
    }
  }
}
