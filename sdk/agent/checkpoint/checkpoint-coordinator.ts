/**
 * Agent Loop Checkpoint Coordinator
 *
 * A service that coordinates the entire checkpoint process
 * Extends BaseCheckpointCoordinator to eliminate code duplication.
 */

import { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import type {
  CheckpointMetadata,
  DeltaStorageConfig,
  TCheckpointType,
  AgentLoopCheckpoint,
  AgentLoopStateSnapshot,
  DeltaCheckpoint,
  FullCheckpoint,
  AgentLoopDelta,
} from "@wf-agent/types";
import { AgentCheckpointError } from "@wf-agent/types";
import { BaseCheckpointCoordinator } from "../../core/checkpoint/base-checkpoint-coordinator.js";
import type { CheckpointDependencies as BaseCheckpointDependencies } from "../../core/checkpoint/types.js";

/**
 * Checkpoint creation options
 */
export interface CheckpointOptions {
  /** Checkpoint metadata */
  metadata?: CheckpointMetadata;
  /** Optional description */
  description?: string;
  /** Optional tags */
  tags?: string[];
}

/**
 * Checkpoint dependencies (extends base with agent-specific fields)
 */
export interface CheckpointDependencies extends BaseCheckpointDependencies<AgentLoopCheckpoint> {
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
 * - Extends BaseCheckpointCoordinator to eliminate duplication
 */
export class AgentLoopCheckpointCoordinator extends BaseCheckpointCoordinator<
  AgentLoopCheckpoint,
  AgentLoopEntity, // Now compatible because CheckpointableEntity only requires 'id'
  AgentLoopStateSnapshot
> {
  /**
   * Create a checkpoint
   * @param entity Agent Loop entity
   * @param dependencies dependencies
   * @param options checkpoint options
   * @returns checkpoint ID
   */
  override async createCheckpoint(
    entity: AgentLoopEntity,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
  ): Promise<string> {
    return await super.createCheckpoint(entity, dependencies, options?.metadata);
  }

  /**
   * Restore Agent Loop entity from checkpoint
   * @param checkpointId checkpointId
   * @param dependencies dependencies
   * @returns Recovered Agent Loop Entity
   */
  override async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies,
  ): Promise<AgentLoopEntity> {
    try {
      return await super.restoreFromCheckpoint(checkpointId, dependencies);
    } catch (error) {
      if (error instanceof Error && error.message.includes("Checkpoint not found")) {
        throw new AgentCheckpointError(
          `Checkpoint not found: ${checkpointId}`,
          "restore",
          checkpointId,
        );
      }
      throw error;
    }
  }

  // ============================================================================
  // Abstract Methods Implementation
  // ============================================================================

  /**
   * Extracting a state snapshot
   * @param entity Agent Loop entity
   * @returns Status Snapshot
   */
  protected extractState(entity: AgentLoopEntity): AgentLoopStateSnapshot {
    return {
      status: entity.state.status,
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      startTime: entity.state.startTime,
      endTime: entity.state.endTime,
      error: entity.state.error,
      messages: entity.getMessages(),
      config: entity.config,
    };
  }

  /**
   * Build checkpoint object
   */
  protected async buildCheckpoint(
    entity: AgentLoopEntity,
    currentState: AgentLoopStateSnapshot,
    checkpointType: TCheckpointType,
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    dependencies: CheckpointDependencies,
    metadata?: CheckpointMetadata,
  ): Promise<AgentLoopCheckpoint> {
    const { getCheckpoint } = dependencies;

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
   * Extract parent ID from checkpoint
   */
  protected extractParentId(checkpoint: AgentLoopCheckpoint): string {
    return checkpoint.agentLoopId;
  }

  /**
   * Create entity from restored state snapshot
   */
  protected createEntityFromSnapshot(parentId: string, snapshot: AgentLoopStateSnapshot): AgentLoopEntity {
    return AgentLoopEntity.fromSnapshot(parentId, snapshot);
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

    // Calculate the difference using inherited diffCalculator
    const delta = this.diffCalculator.calculateDelta(baseCheckpoint.snapshot, currentState);

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
   * Determine the checkpoint type
   * 
   * Agent-specific strategy: Creates FULL checkpoint when (checkpointCount + 1) is divisible by baselineInterval.
   * This means: with baselineInterval=5, checkpoints at count 4, 9, 14... will be FULL.
   * 
   * @param checkpointCount The current number of checkpoints
   * @param config Incremental storage configuration
   * @returns The type of checkpoint
   */
  protected override determineCheckpointType(
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
    // Agent uses (checkpointCount + 1) to align with iteration-based semantics
    if ((checkpointCount + 1) % config.baselineInterval === 0) {
      return "FULL";
    }

    // Create incremental checkpoints in other cases
    return "DELTA";
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
   * Verify checkpoint integrity and compatibility
   */
  protected override validateCheckpoint(checkpoint: AgentLoopCheckpoint): void {
    // Call parent validation first
    super.validateCheckpoint(checkpoint);

    // Additional agent-specific validation
    if (!checkpoint.agentLoopId) {
      throw new AgentCheckpointError(
        "Invalid checkpoint: missing agentLoopId",
        "validate",
        checkpoint.id,
        checkpoint.agentLoopId,
      );
    }

    // Validation against checkpoint type
    if (checkpoint.type === "DELTA") {
      const deltaCheckpoint = checkpoint as DeltaCheckpoint<AgentLoopDelta>;
      if (!deltaCheckpoint.delta && !deltaCheckpoint.previousCheckpointId) {
        throw new AgentCheckpointError(
          "Invalid delta checkpoint: missing delta data and previous checkpoint reference",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }
    } else {
      const fullCheckpoint = checkpoint as FullCheckpoint<AgentLoopStateSnapshot>;
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
