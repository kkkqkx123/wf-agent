/**
 * Agent Loop Delta Restorer
 *
 * Implements Agent Loop specific delta restoration based on the generic DeltaRestorer.
 */

import {
  DeltaRestorer,
  createCheckpointLoader,
  type RestoreResult,
} from "../../core/utils/checkpoint/delta-restorer.js";
import type { AgentLoopCheckpoint, AgentLoopStateSnapshot, AgentLoopDelta } from "@wf-agent/types";

/**
 * Agent Loop restore result
 * Extends the base RestoreResult with Agent Loop specific metadata
 */
export interface AgentLoopRestoreResult extends RestoreResult<AgentLoopStateSnapshot> {
  snapshot: AgentLoopStateSnapshot;
  metadata: {
    messages: unknown[];
    variables: Record<string, unknown>;
    config: unknown;
  };
}

/**
 * Agent Loop Delta Restorer
 *
 * Extends the generic DeltaRestorer with Agent Loop specific restoration logic.
 */
export class AgentLoopDeltaRestorer extends DeltaRestorer<
  AgentLoopCheckpoint,
  AgentLoopStateSnapshot,
  AgentLoopDelta,
  AgentLoopRestoreResult
> {
  constructor(
    loadCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>,
    listCheckpoints: (_agentLoopId: string) => Promise<string[]>,
  ) {
    super(
      createCheckpointLoader<AgentLoopCheckpoint>({
        load: loadCheckpoint,
        list: listCheckpoints,
      }),
    );
  }

  /**
   * Create restore result with Agent Loop specific metadata
   */
  protected override createRestoreResult(snapshot: AgentLoopStateSnapshot): AgentLoopRestoreResult {
    return {
      snapshot,
      metadata: {
        messages: snapshot.messages,
        variables: snapshot.variables,
        config: snapshot.config,
      },
    };
  }

  /**
   * Extract snapshot from checkpoint
   */
  protected extractSnapshot(checkpoint: AgentLoopCheckpoint): AgentLoopStateSnapshot {
    if (!checkpoint.snapshot) {
      throw new Error(`Checkpoint ${checkpoint.id} has no snapshot`);
    }
    return checkpoint.snapshot;
  }

  /**
   * Check if checkpoint has snapshot
   */
  protected hasSnapshot(checkpoint: AgentLoopCheckpoint): boolean {
    return !!checkpoint.snapshot;
  }

  /**
   * Extract parent ID from checkpoint
   */
  protected extractParentId(checkpoint: AgentLoopCheckpoint): string {
    return checkpoint.agentLoopId;
  }

  /**
   * Apply delta to snapshot
   */
  protected applyDelta(
    snapshot: AgentLoopStateSnapshot,
    delta: AgentLoopDelta,
  ): AgentLoopStateSnapshot {
    const result: AgentLoopStateSnapshot = {
      ...snapshot,
      messages: [...snapshot.messages],
      variables: { ...snapshot.variables },
    };

    // Apply added messages
    if (delta.addedMessages && delta.addedMessages.length > 0) {
      result.messages.push(...delta.addedMessages);
    }

    // Apply status change
    if (delta.statusChange) {
      result.status = delta.statusChange.to;
    }

    // Apply modified variables
    if (delta.modifiedVariables) {
      for (const [key, value] of delta.modifiedVariables) {
        result.variables[key] = value;
      }
    }

    // Apply other changes
    if (delta.otherChanges) {
      for (const [key, change] of Object.entries(delta.otherChanges)) {
        result[key] = (change as { to: unknown }).to;
      }
    }

    return result;
  }
}
