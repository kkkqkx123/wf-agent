/**
 * Checkpoint Serializer
 *
 * Handles serialization and deserialization of Checkpoint snapshots.
 */

import { Serializer } from "../serializer.js";
import { DeltaCalculator } from "../delta-calculator.js";
import { SerializationRegistry } from "../serialization-registry.js";
import type {
  SnapshotBase,
  Checkpoint,
  CheckpointDelta,
  ThreadStateSnapshot,
} from "@wf-agent/types";

/**
 * Checkpoint Snapshot
 *
 * Extends SnapshotBase with Checkpoint-specific fields.
 */
export interface CheckpointSnapshot extends SnapshotBase {
  _entityType: "checkpoint";
  /** The underlying checkpoint data */
  checkpoint: Checkpoint;
}

/**
 * Checkpoint Snapshot Serializer
 */
export class CheckpointSnapshotSerializer extends Serializer<CheckpointSnapshot> {
  constructor() {
    super({ prettyPrint: true, targetVersion: 1 });
  }

  /**
   * Serialize a Checkpoint directly to Uint8Array
   */
  serializeCheckpoint(checkpoint: Checkpoint): Uint8Array {
    const snapshot: CheckpointSnapshot = {
      _version: 1,
      _timestamp: Date.now(),
      _entityType: "checkpoint",
      checkpoint,
    };
    return this.serialize(snapshot);
  }

  /**
   * Deserialize to a Checkpoint directly
   */
  deserializeCheckpoint(data: Uint8Array): Checkpoint {
    const snapshot = this.deserialize(data);
    return snapshot.checkpoint;
  }
}

/**
 * Checkpoint Delta Calculator
 */
export class CheckpointDeltaCalculator extends DeltaCalculator<CheckpointSnapshot> {
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

    if (current.threadState && previous.threadState) {
      const stateDelta = this.computeThreadStateDelta(previous.threadState, current.threadState);

      if (stateDelta && Object.keys(stateDelta).length > 0) {
        delta.addedMessages = stateDelta.addedMessages;
        delta.modifiedMessages = stateDelta.modifiedMessages;
        delta.deletedMessageIndices = stateDelta.deletedMessageIndices;
        delta.addedVariables = stateDelta.addedVariables;
        delta.modifiedVariables = stateDelta.modifiedVariables;
      }
    }

    if (previous.threadState?.status !== current.threadState?.status) {
      delta.statusChange = {
        from: previous.threadState?.status as import("@wf-agent/types").ThreadStatus,
        to: current.threadState?.status as import("@wf-agent/types").ThreadStatus,
      };
    }

    return Object.keys(delta).length > 0 ? delta : null;
  }

  /**
   * Compute delta between two thread states
   */
  private computeThreadStateDelta(
    previous: ThreadStateSnapshot,
    current: ThreadStateSnapshot,
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
 * Register Checkpoint serializer with the global registry
 */
export function registerCheckpointSerializer(): void {
  const registry = SerializationRegistry.getInstance();

  registry.register({
    entityType: "checkpoint",
    serializer: new CheckpointSnapshotSerializer(),
    deltaCalculator: new CheckpointDeltaCalculator(),
  });
}
