/**
 * Checkpoint Delta Restorer
 *
 * Implements Graph/Thread specific delta restoration based on the generic DeltaRestorer.
 */

import {
  DeltaRestorer,
  createCheckpointLoader,
} from "../../../core/utils/checkpoint/delta-restorer.js";
import type { Checkpoint, ThreadStateSnapshot, CheckpointDelta } from "@wf-agent/types";

/**
 * Checkpoint Delta Restorer
 *
 * Extends the generic DeltaRestorer with Graph/Thread specific restoration logic.
 */
export class DeltaCheckpointRestorer extends DeltaRestorer<
  Checkpoint,
  ThreadStateSnapshot,
  CheckpointDelta
> {
  constructor(
    loadCheckpoint: (id: string) => Promise<Checkpoint | null>,
    listCheckpoints: (threadId: string) => Promise<string[]>,
  ) {
    super(
      createCheckpointLoader<Checkpoint>({
        load: loadCheckpoint,
        list: listCheckpoints,
      }),
    );
  }

  /**
   * Extract snapshot from checkpoint
   */
  protected extractSnapshot(checkpoint: Checkpoint): ThreadStateSnapshot {
    if (!checkpoint.threadState) {
      throw new Error(`Checkpoint ${checkpoint.id} has no thread state`);
    }
    return checkpoint.threadState;
  }

  /**
   * Check if checkpoint has snapshot
   */
  protected hasSnapshot(checkpoint: Checkpoint): boolean {
    return !!checkpoint.threadState;
  }

  /**
   * Extract parent ID from checkpoint
   */
  protected extractParentId(checkpoint: Checkpoint): string {
    return checkpoint.threadId;
  }

  /**
   * Apply delta to snapshot
   */
  protected applyDelta(snapshot: ThreadStateSnapshot, delta: CheckpointDelta): ThreadStateSnapshot {
    const result: ThreadStateSnapshot = {
      ...snapshot,
      conversationState: {
        ...snapshot.conversationState,
        messages: [...snapshot.conversationState.messages],
      },
      variables: { ...snapshot.variables },
      nodeResults: { ...snapshot.nodeResults },
    };

    // Apply added messages
    if (delta.addedMessages && delta.addedMessages.length > 0) {
      result.conversationState.messages.push(...delta.addedMessages);
    }

    // Apply added variables
    if (delta.addedVariables && delta.addedVariables.length > 0) {
      result.variables.push(...delta.addedVariables);
    }

    // Apply modified variables
    if (delta.modifiedVariables) {
      for (const [variableName, value] of delta.modifiedVariables) {
        (result.variables as unknown as Record<string, unknown>)[variableName] = value;
      }
    }

    // Apply added node results
    if (delta.addedNodeResults) {
      for (const [nodeId, nodeResult] of Object.entries(delta.addedNodeResults)) {
        result.nodeResults[nodeId] = nodeResult;
      }
    }

    // Apply status change
    if (delta.statusChange) {
      result.status = delta.statusChange.to;
    }

    // Apply current node change
    if (delta.currentNodeChange) {
      result.currentNodeId = delta.currentNodeChange.to;
    }

    // Apply other changes (output, errors, etc.)
    if (delta.otherChanges) {
      for (const [key, change] of Object.entries(delta.otherChanges)) {
        if (change && typeof change === "object" && "to" in change) {
          (result as unknown as Record<string, unknown>)[key] = change.to;
        }
      }
    }

    return result;
  }
}
