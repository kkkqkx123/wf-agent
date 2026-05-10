/**
 * Agent Loop Checkpoint Snapshot Types
 *
 * Defines Agent Loop Checkpoint snapshot structure and delta calculation utilities.
 */

import type {
  SnapshotBase,
  AgentLoopCheckpoint,
  AgentLoopDelta,
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
 * Utility functions for Agent Loop Checkpoint delta calculation
 */
export const AgentLoopCheckpointDeltaUtils = {
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

    const delta = computeCheckpointDelta(previous, current);

    if (!delta || Object.keys(delta).length === 0) {
      return { type: "FULL", snapshot: current };
    }

    return { type: "DELTA", delta };
  },
};

/**
 * Compute delta between two agent loop checkpoints
 */
function computeCheckpointDelta(
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
