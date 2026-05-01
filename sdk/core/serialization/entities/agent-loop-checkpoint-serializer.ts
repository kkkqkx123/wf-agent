/**
 * Agent Loop Checkpoint Serializer
 *
 * Handles serialization and deserialization of Agent Loop Checkpoint snapshots.
 */

import { Serializer } from "../serializer.js";
import { DeltaCalculator } from "../delta-calculator.js";
import { SerializationRegistry } from "../serialization-registry.js";
import type {
  SnapshotBase,
  AgentLoopCheckpoint,
  AgentLoopDelta,
  AgentLoopStateSnapshot,
} from "@wf-agent/types";

/**
 * Agent Loop Checkpoint Snapshot
 *
 * Extends SnapshotBase with Agent Loop Checkpoint-specific fields.
 */
export interface AgentLoopCheckpointSnapshot extends SnapshotBase {
  _entityType: "agentLoopCheckpoint";
  /** The underlying agent loop checkpoint data */
  checkpoint: AgentLoopCheckpoint;
}

/**
 * Agent Loop Checkpoint Snapshot Serializer
 */
export class AgentLoopCheckpointSerializer extends Serializer<AgentLoopCheckpointSnapshot> {
  constructor() {
    super({ prettyPrint: true, targetVersion: 1 });
  }

  /**
   * Serialize an Agent Loop Checkpoint directly to Uint8Array
   */
  async serializeCheckpoint(checkpoint: AgentLoopCheckpoint): Promise<Uint8Array> {
    const snapshot: AgentLoopCheckpointSnapshot = {
      _version: 1,
      _timestamp: Date.now(),
      _entityType: "agentLoopCheckpoint",
      checkpoint,
    };
    return this.serialize(snapshot);
  }

  /**
   * Deserialize to an Agent Loop Checkpoint directly
   */
  async deserializeCheckpoint(data: Uint8Array): Promise<AgentLoopCheckpoint> {
    const snapshot = await this.deserialize(data);
    if (snapshot._entityType !== "agentLoopCheckpoint") {
      throw new Error(`Expected agentLoopCheckpoint, got ${snapshot._entityType}`);
    }
    return snapshot.checkpoint;
  }
}

/**
 * Agent Loop Checkpoint Delta Calculator
 */
export class AgentLoopCheckpointDeltaCalculator extends DeltaCalculator<AgentLoopCheckpointSnapshot> {
  constructor() {
    super({
      deepCompare: true,
      ignoreFields: ["_timestamp"],
    });
  }

  /**
   * Calculate delta between two agent loop checkpoints
   */
  calculateCheckpointDelta(
    previous: AgentLoopCheckpoint | null,
    current: AgentLoopCheckpoint,
  ): { type: "FULL" | "DELTA"; snapshot?: AgentLoopCheckpoint; delta?: AgentLoopDelta } {
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
   * Compute delta between two agent loop checkpoints
   */
  private computeCheckpointDelta(
    previous: AgentLoopCheckpoint,
    current: AgentLoopCheckpoint,
  ): AgentLoopDelta | null {
    const delta: AgentLoopDelta = {};

    // Access snapshot from FullCheckpoint or skip for DeltaCheckpoint
    const previousSnapshot = previous.type === "FULL" ? previous.snapshot : undefined;
    const currentSnapshot = current.type === "FULL" ? current.snapshot : undefined;

    if (currentSnapshot && previousSnapshot) {
      // Calculate message changes
      if (currentSnapshot.messages && previousSnapshot.messages) {
        const prevMessages = previousSnapshot.messages;
        const currMessages = currentSnapshot.messages;

        if (currMessages.length > prevMessages.length) {
          delta.addedMessages = currMessages.slice(prevMessages.length);
        }
      }

      // Calculate variable changes
      if (currentSnapshot.variables && previousSnapshot.variables) {
        const modifiedVariables = new Map<string, unknown>();
        for (const [key, value] of Object.entries(currentSnapshot.variables)) {
          if (JSON.stringify(previousSnapshot.variables[key]) !== JSON.stringify(value)) {
            modifiedVariables.set(key, value);
          }
        }
        if (modifiedVariables.size > 0) {
          delta.modifiedVariables = modifiedVariables;
        }
      }

      // Calculate iteration history changes
      if (
        currentSnapshot.iterationHistory &&
        previousSnapshot.iterationHistory &&
        currentSnapshot.iterationHistory.length > previousSnapshot.iterationHistory.length
      ) {
        delta.addedIterations = currentSnapshot.iterationHistory.slice(
          previousSnapshot.iterationHistory.length,
        );
      }

      // Calculate status change
      if (previousSnapshot.status !== currentSnapshot.status) {
        delta.statusChange = {
          from: previousSnapshot.status,
          to: currentSnapshot.status,
        };
      }

      // Calculate other state changes
      const otherChanges: Record<string, { from: unknown; to: unknown }> = {};

      if (previousSnapshot.currentIteration !== currentSnapshot.currentIteration) {
        otherChanges["currentIteration"] = {
          from: previousSnapshot.currentIteration,
          to: currentSnapshot.currentIteration,
        };
      }

      if (previousSnapshot.toolCallCount !== currentSnapshot.toolCallCount) {
        otherChanges["toolCallCount"] = {
          from: previousSnapshot.toolCallCount,
          to: currentSnapshot.toolCallCount,
        };
      }

      if (Object.keys(otherChanges).length > 0) {
        delta.otherChanges = otherChanges;
      }
    }

    return Object.keys(delta).length > 0 ? delta : null;
  }
}

/**
 * Register Agent Loop Checkpoint serializer with the global registry
 */
export function registerAgentLoopCheckpointSerializer(): void {
  const registry = SerializationRegistry.getInstance();

  registry.register({
    entityType: "agentLoopCheckpoint",
    serializer: new AgentLoopCheckpointSerializer(),
    deltaCalculator: new AgentLoopCheckpointDeltaCalculator(),
  });
}
