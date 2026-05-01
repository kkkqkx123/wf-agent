/**
 * Workflow Checkpoint Serializer
 *
 * Handles serialization and deserialization of Workflow Checkpoint snapshots.
 */

import { Serializer } from "../serializer.js";
import { DeltaCalculator } from "../delta-calculator.js";
import { SerializationRegistry } from "../serialization-registry.js";
import type {
  SnapshotBase,
  Checkpoint,
  CheckpointDelta,
  WorkflowExecutionStateSnapshot,
} from "@wf-agent/types";

/**
 * Workflow Checkpoint Snapshot
 *
 * Extends SnapshotBase with Workflow Checkpoint-specific fields.
 */
export interface WorkflowCheckpointSnapshot extends SnapshotBase {
  _entityType: "workflowCheckpoint";
  /** The underlying workflow checkpoint data */
  checkpoint: Checkpoint;
}

/**
 * Workflow Checkpoint Snapshot Serializer
 */
export class WorkflowCheckpointSerializer extends Serializer<WorkflowCheckpointSnapshot> {
  constructor() {
    super({ prettyPrint: true, targetVersion: 1 });
  }

  /**
   * Serialize a Checkpoint directly to Uint8Array
   */
  async serializeCheckpoint(checkpoint: Checkpoint): Promise<Uint8Array> {
    const snapshot: WorkflowCheckpointSnapshot = {
      _version: 1,
      _timestamp: Date.now(),
      _entityType: "workflowCheckpoint",
      checkpoint,
    };
    return this.serialize(snapshot);
  }

  /**
   * Deserialize to a Checkpoint directly
   */
  async deserializeCheckpoint(data: Uint8Array): Promise<Checkpoint> {
    const snapshot = await this.deserialize(data);
    return snapshot.checkpoint;
  }
}

/**
 * Workflow Checkpoint Delta Calculator
 */
export class WorkflowCheckpointDeltaCalculator extends DeltaCalculator<WorkflowCheckpointSnapshot> {
  constructor() {
    super({
      deepCompare: true,
      ignoreFields: ["_timestamp"],
    });
  }

  /**
   * Calculate delta between two checkpoints
   */
  calculateCheckpointDelta(
    previous: Checkpoint | null,
    current: Checkpoint,
  ): { type: "FULL" | "DELTA"; snapshot?: Checkpoint; delta?: CheckpointDelta } {
    if (!previous) {
      return { type: "FULL", snapshot: current };
    }

    const delta = this.computeCheckpointDelta(previous, current);

    if (!delta || Object.keys(delta).length === 0) {
      return { type: "FULL", snapshot: current };
    }

    return { type: "DELTA", delta };
  }

  /**
   * Compute delta between two checkpoints
   */
  private computeCheckpointDelta(
    previous: Checkpoint,
    current: Checkpoint,
  ): CheckpointDelta | null {
    const delta: CheckpointDelta = {};

    // Access snapshot from FullCheckpoint or skip for DeltaCheckpoint
    const previousSnapshot = previous.type === "FULL" ? previous.snapshot : undefined;
    const currentSnapshot = current.type === "FULL" ? current.snapshot : undefined;

    if (currentSnapshot && previousSnapshot) {
      const stateDelta = this.computeExecutionStateDelta(previousSnapshot, currentSnapshot);

      if (stateDelta && Object.keys(stateDelta).length > 0) {
        delta.addedMessages = stateDelta.addedMessages;
        delta.modifiedMessages = stateDelta.modifiedMessages;
        delta.deletedMessageIndices = stateDelta.deletedMessageIndices;
        delta.addedVariables = stateDelta.addedVariables;
        delta.modifiedVariables = stateDelta.modifiedVariables;
      }
    }

    if (previousSnapshot?.status !== currentSnapshot?.status) {
      delta.statusChange = {
        from: previousSnapshot?.status as import("@wf-agent/types").WorkflowExecutionStatus,
        to: currentSnapshot?.status as import("@wf-agent/types").WorkflowExecutionStatus,
      };
    }

    return Object.keys(delta).length > 0 ? delta : null;
  }

  /**
   * Compute delta between two execution states
   */
  private computeExecutionStateDelta(
    previous: WorkflowExecutionStateSnapshot,
    current: WorkflowExecutionStateSnapshot,
  ): Partial<CheckpointDelta> | null {
    const delta: Partial<CheckpointDelta> = {};

    if (current.conversationState?.messages && previous.conversationState?.messages) {
      const prevMessages = previous.conversationState.messages;
      const currMessages = current.conversationState.messages;

      if (currMessages.length > prevMessages.length) {
        delta.addedMessages = currMessages.slice(prevMessages.length);
      }
    }

    return Object.keys(delta).length > 0 ? delta : null;
  }
}

/**
 * Register Workflow Checkpoint serializer with the global registry
 */
export function registerWorkflowCheckpointSerializer(): void {
  const registry = SerializationRegistry.getInstance();

  registry.register({
    entityType: "workflowCheckpoint",
    serializer: new WorkflowCheckpointSerializer(),
    deltaCalculator: new WorkflowCheckpointDeltaCalculator(),
  });
}

/**
 * @deprecated Use WorkflowCheckpointSnapshot instead
 */
export type CheckpointSnapshot = WorkflowCheckpointSnapshot;

/**
 * @deprecated Use WorkflowCheckpointSerializer instead
 */
export const CheckpointSnapshotSerializer = WorkflowCheckpointSerializer;

/**
 * @deprecated Use WorkflowCheckpointDeltaCalculator instead
 */
export const CheckpointDeltaCalculator = WorkflowCheckpointDeltaCalculator;

/**
 * @deprecated Use registerWorkflowCheckpointSerializer instead
 */
export const registerCheckpointSerializer = registerWorkflowCheckpointSerializer;
