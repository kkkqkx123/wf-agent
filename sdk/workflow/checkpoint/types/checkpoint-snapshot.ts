/**
 * Workflow Checkpoint Snapshot Types
 *
 * Defines Workflow Checkpoint snapshot structure.
 */

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
 * Utility functions for Workflow Checkpoint delta calculation
 */
export const WorkflowCheckpointDeltaUtils = {
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

    const delta = computeCheckpointDelta(previous, current);

    if (!delta || Object.keys(delta).length === 0) {
      return { type: "FULL", snapshot: current };
    }

    return { type: "DELTA", delta };
  },
};

/**
 * Compute delta between two checkpoints
 */
function computeCheckpointDelta(
  previous: Checkpoint,
  current: Checkpoint,
): CheckpointDelta | null {
  const delta: CheckpointDelta = {};

  // Access snapshot from FullCheckpoint or skip for DeltaCheckpoint
  const previousSnapshot = previous.type === "FULL" ? previous.snapshot : undefined;
  const currentSnapshot = current.type === "FULL" ? current.snapshot : undefined;

  if (currentSnapshot && previousSnapshot) {
    const stateDelta = computeExecutionStateDelta(previousSnapshot, currentSnapshot);

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
function computeExecutionStateDelta(
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
