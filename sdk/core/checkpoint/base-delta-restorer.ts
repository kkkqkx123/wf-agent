/**
 * Base Delta Restorer
 * 
 * Handles restoration of full state from delta checkpoint chains.
 * Traverses the delta chain backward to find the base checkpoint,
 * then applies deltas forward to reconstruct the complete state.
 */

import type { BaseCheckpoint } from "@wf-agent/types";
import { BaseDiffCalculator } from "./base-diff-calculator.js";
import type { DeltaRestoreResult } from "./types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "BaseDeltaRestorer" });

/**
 * Base Delta Restorer
 * 
 * Handles restoration of full state from delta checkpoint chains.
 * Traverses the delta chain backward to find the base checkpoint,
 * then applies deltas forward to reconstruct the complete state.
 * 
 * @template TCheckpoint - The checkpoint type
 * @template TState - The state snapshot type
 */
export class BaseDeltaRestorer<
  TCheckpoint extends BaseCheckpoint<unknown, unknown>,
  TState
> {
  private diffCalculator: BaseDiffCalculator;
  private loadCheckpoint: (id: string) => Promise<TCheckpoint | null>;

  constructor(
    loadCheckpoint: (id: string) => Promise<TCheckpoint | null>
  ) {
    this.diffCalculator = new BaseDiffCalculator();
    this.loadCheckpoint = loadCheckpoint;
  }

  /**
   * Restore full state from a checkpoint (handles both FULL and DELTA)
   * @param checkpointId The checkpoint ID to restore from
   * @param entityId Optional entity ID for the checkpoint (used for batch loading optimization)
   * @returns Restoration result with full snapshot
   */
  async restore(
    checkpointId: string,
    entityId?: string
  ): Promise<DeltaRestoreResult<TState>> {
    logger.debug("Starting checkpoint restoration", { checkpointId, entityId });

    // Load the target checkpoint
    const targetCheckpoint = await this.loadCheckpoint(checkpointId);
    if (!targetCheckpoint) {
      throw new Error(`Checkpoint not found: ${checkpointId}`);
    }

    // If it's a full checkpoint, return directly
    if (targetCheckpoint.type === "FULL") {
      logger.debug("Restoring from full checkpoint", { checkpointId });
      return {
        snapshot: targetCheckpoint.snapshot as TState,
        metadata: {
          checkpointChain: [checkpointId],
          baseCheckpointId: checkpointId,
        },
      };
    }

    // For delta checkpoint, traverse the chain to find base
    logger.debug("Restoring from delta checkpoint, traversing chain", {
      checkpointId,
      entityId,
      baseCheckpointId: targetCheckpoint.baseCheckpointId,
    });

    // Build chain by tracing backward: target → base (sequential loads, only chain members)
    const chain = await this.buildCheckpointChain(checkpointId, entityId);
    const baseCheckpointId = chain[0];

    if (!baseCheckpointId) {
      throw new Error(`Empty checkpoint chain for: ${checkpointId}`);
    }

    // Load base checkpoint
    const baseCheckpoint = await this.loadCheckpoint(baseCheckpointId);
    if (!baseCheckpoint || baseCheckpoint.type !== "FULL") {
      throw new Error(`Base checkpoint not found or invalid: ${baseCheckpointId}`);
    }

    // Start with base snapshot
    let currentSnapshot = baseCheckpoint.snapshot as TState;

    // Apply deltas forward through the chain
    for (let i = 1; i < chain.length; i++) {
      const deltaCheckpointId = chain[i];
      if (!deltaCheckpointId) continue;
      
      const deltaCheckpoint = await this.loadCheckpoint(deltaCheckpointId);

      if (!deltaCheckpoint || deltaCheckpoint.type !== "DELTA") {
        throw new Error(`Invalid delta checkpoint in chain: ${deltaCheckpointId}`);
      }

      // Apply delta to current snapshot
      currentSnapshot = this.diffCalculator.applyDelta(
        currentSnapshot as Record<string, unknown>,
        deltaCheckpoint.delta as Record<string, { from: unknown; to: unknown }>
      ) as TState;

      logger.debug("Applied delta", {
        from: deltaCheckpoint.previousCheckpointId,
        to: deltaCheckpointId,
      });
    }

    logger.info("Checkpoint restoration completed", {
      checkpointId,
      chainLength: chain.length,
      baseCheckpointId,
    });

    return {
      snapshot: currentSnapshot,
      metadata: {
        checkpointChain: chain,
        baseCheckpointId,
      },
    };
  }

  /**
   * Build checkpoint chain from target to base via sequential backward traversal
   * 
   * Traces the `previousCheckpointId` links from target checkpoint backward
   * until a FULL checkpoint is found. Only loads checkpoints that are
   * actually in the chain — no over-fetching of unrelated checkpoints.
   * 
   * @param checkpointId Starting checkpoint ID (must be a DELTA checkpoint)
   * @param entityId Optional entity ID (used for logging diagnostics)
   * @returns Array of checkpoint IDs from base to target
   */
  private async buildCheckpointChain(
    checkpointId: string,
    entityId?: string
  ): Promise<string[]> {
    const chain: string[] = [];
    let currentId: string | undefined = checkpointId;
    const visited = new Set<string>();

    while (currentId) {
      // Prevent infinite loops
      if (visited.has(currentId)) {
        throw new Error(`Circular reference detected in checkpoint chain at: ${currentId}`);
      }
      visited.add(currentId);

      // Load the checkpoint
      const checkpoint = await this.loadCheckpoint(currentId);
      if (!checkpoint) {
        throw new Error(`Checkpoint not found in chain: ${currentId}`);
      }

      chain.unshift(currentId); // Add to beginning (base first)

      if (checkpoint.type === "FULL") {
        // Reached base checkpoint
        break;
      }

      // Move to previous checkpoint via link
      currentId = checkpoint.previousCheckpointId;
    }

    logger.debug("Checkpoint chain built", {
      chainLength: chain.length,
      baseCheckpointId: chain[0],
      targetCheckpointId: chain[chain.length - 1],
      entityId,
    });

    return chain;
  }
}
