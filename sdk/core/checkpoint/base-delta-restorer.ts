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
  private listCheckpoints: (parentId: string) => Promise<string[]>;

  constructor(
    loadCheckpoint: (id: string) => Promise<TCheckpoint | null>,
    listCheckpoints: (parentId: string) => Promise<string[]>
  ) {
    this.diffCalculator = new BaseDiffCalculator();
    this.loadCheckpoint = loadCheckpoint;
    this.listCheckpoints = listCheckpoints;
  }

  /**
   * Restore full state from a checkpoint (handles both FULL and DELTA)
   * @param checkpointId The checkpoint ID to restore from
   * @returns Restoration result with full snapshot
   */
  async restore(checkpointId: string): Promise<DeltaRestoreResult<TState>> {
    logger.debug("Starting checkpoint restoration", { checkpointId });

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
      baseCheckpointId: targetCheckpoint.baseCheckpointId,
    });

    const chain = await this.buildCheckpointChain(checkpointId);
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
   * Build checkpoint chain from target to base
   * Uses listCheckpoints to batch-load checkpoints for better performance
   * @param checkpointId Starting checkpoint ID
   * @returns Array of checkpoint IDs from base to target
   */
  private async buildCheckpointChain(checkpointId: string): Promise<string[]> {
    // Step 1: Load target checkpoint to get its parent ID
    const targetCheckpoint = await this.loadCheckpoint(checkpointId);
    if (!targetCheckpoint) {
      throw new Error(`Checkpoint not found: ${checkpointId}`);
    }

    // If it's a full checkpoint, return single-element chain
    if (targetCheckpoint.type === "FULL") {
      return [checkpointId];
    }

    // Step 2: Get parent ID from delta checkpoint
    const parentId = targetCheckpoint.previousCheckpointId;
    if (!parentId) {
      throw new Error(`Delta checkpoint missing previousCheckpointId: ${checkpointId}`);
    }

    // Step 3: Batch load all checkpoints under the same parent
    // This reduces I/O from N sequential loads to 1 batch load + individual loads for chain traversal
    logger.debug("Batch loading checkpoints for chain construction", { parentId });
    const allCheckpointIds = await this.listCheckpoints(parentId);
    
    // Step 4: Load all checkpoints in parallel for efficiency
    const checkpointMap = new Map<string, TCheckpoint>();
    const loadPromises = allCheckpointIds.map(async (id) => {
      const cp = await this.loadCheckpoint(id);
      if (cp) {
        checkpointMap.set(id, cp);
      }
    });
    await Promise.all(loadPromises);

    // Step 5: Build chain by traversing backward through previousCheckpointId links
    const chain: string[] = [];
    let currentId: string | undefined = checkpointId;
    const visited = new Set<string>();

    while (currentId) {
      // Prevent infinite loops
      if (visited.has(currentId)) {
        throw new Error(`Circular reference detected in checkpoint chain at: ${currentId}`);
      }
      visited.add(currentId);

      chain.unshift(currentId); // Add to beginning

      const checkpoint = checkpointMap.get(currentId);
      if (!checkpoint) {
        throw new Error(`Checkpoint not found in chain: ${currentId}`);
      }

      if (checkpoint.type === "FULL") {
        // Reached base checkpoint
        break;
      }

      // Move to previous checkpoint
      currentId = checkpoint.previousCheckpointId;
    }

    logger.debug("Checkpoint chain built", {
      chainLength: chain.length,
      baseCheckpointId: chain[0],
      targetCheckpointId: chain[chain.length - 1],
    });

    return chain;
  }
}
