/**
 * Generic Delta Restorer
 *
 * Provides generic incremental recovery logic that can be reused by Graph and Agent modules.
 */

import { CheckpointType } from "@wf-agent/types";
import type { BaseCheckpoint } from "@wf-agent/types";

// Re-export BaseCheckpoint for backward compatibility
export type { BaseCheckpoint };

/**
 * Restore result interface
 * Wraps the restored snapshot with optional metadata
 */
export interface RestoreResult<TSnapshot> {
  /** Restored snapshot */
  snapshot: TSnapshot;
  /** Additional metadata (e.g., messages, variables extracted from snapshot) */
  metadata?: Record<string, unknown>;
}

/**
 * Checkpoint loader interface
 */
export interface CheckpointLoader<TCheckpoint extends BaseCheckpoint<unknown, unknown>> {
  load(checkpointId: string): Promise<TCheckpoint | null>;
  list(parentId: string): Promise<string[]>;
}

/**
 * Generic Delta Restorer
 *
 * @template TCheckpoint Checkpoint type
 * @template TSnapshot Snapshot type
 * @template TDelta Delta type
 * @template TRestoreResult Restore result type (defaults to RestoreResult<TSnapshot>)
 */
export abstract class DeltaRestorer<
  TCheckpoint extends BaseCheckpoint<TDelta, TSnapshot>,
  TSnapshot,
  TDelta,
  TRestoreResult extends RestoreResult<TSnapshot> = RestoreResult<TSnapshot>,
> {
  protected loader: CheckpointLoader<TCheckpoint>;

  constructor(loader: CheckpointLoader<TCheckpoint>) {
    this.loader = loader;
  }

  /**
   * Restore full snapshot from checkpoint
   *
   * @param checkpointId Checkpoint ID
   * @returns Restore result containing snapshot and optional metadata
   */
  async restore(checkpointId: string): Promise<TRestoreResult> {
    const checkpoint = await this.loader.load(checkpointId);
    if (!checkpoint) {
      throw new Error(`Checkpoint not found: ${checkpointId}`);
    }

    // If it's a full checkpoint, return the snapshot directly.
    if (!checkpoint.type || checkpoint.type === CheckpointType["FULL"]) {
      const snapshot = this.extractSnapshot(checkpoint);
      return this.createRestoreResult(snapshot);
    }

    // If it is an incremental checkpoint, chain recovery is required.
    const snapshot = await this.restoreDeltaCheckpoint(checkpoint);
    return this.createRestoreResult(snapshot);
  }

  /**
   * Restore incremental checkpoint
   *
   * @param deltaCheckpoint Incremental checkpoint
   * @returns Full snapshot
   */
  protected async restoreDeltaCheckpoint(deltaCheckpoint: TCheckpoint): Promise<TSnapshot> {
    // 1. Find the baseline checkpoint
    const baseCheckpoint = await this.findBaseCheckpoint(deltaCheckpoint);

    // 2. Build the delta chain
    const deltaChain = await this.buildDeltaChain(baseCheckpoint.id, deltaCheckpoint.id);

    // 3. Apply delta sequentially
    let snapshot = this.extractSnapshot(baseCheckpoint);
    for (const delta of deltaChain) {
      snapshot = this.applyDelta(snapshot, delta);
    }

    return snapshot;
  }

  /**
   * Find the baseline checkpoint
   *
   * @param checkpoint Starting checkpoint
   * @returns Baseline checkpoint
   */
  protected async findBaseCheckpoint(checkpoint: TCheckpoint): Promise<TCheckpoint> {
    // If there is a baseCheckpointId, load it directly.
    if (checkpoint.baseCheckpointId) {
      const base = await this.loader.load(checkpoint.baseCheckpointId);
      if (base && this.hasSnapshot(base)) {
        return base;
      }
    }

    // Otherwise, find the most recent full checkpoint
    const parentId = this.extractParentId(checkpoint);
    const checkpointIds = await this.loader.list(parentId);

    // Load all checkpoints and find the full checkpoint
    const checkpoints: TCheckpoint[] = [];
    for (const id of checkpointIds) {
      const cp = await this.loader.load(id);
      if (cp) {
        checkpoints.push(cp);
      }
    }

    // Sort by timestamp in descending order
    checkpoints.sort((a, b) => (b.timestamp || 0) - (a.timestamp || 0));

    for (const cp of checkpoints) {
      if (!cp.type || cp.type === CheckpointType["FULL"]) {
        return cp;
      }
    }

    throw new Error(`No base checkpoint found for delta checkpoint: ${checkpoint.id}`);
  }

  /**
   * Build the delta chain
   *
   * @param baseId Baseline checkpoint ID
   * @param targetId Target checkpoint ID
   * @returns Delta array
   */
  protected async buildDeltaChain(baseId: string, targetId: string): Promise<TDelta[]> {
    const deltas: TDelta[] = [];
    const checkpoint = await this.loader.load(targetId);

    if (!checkpoint || checkpoint.id === baseId) {
      return deltas;
    }

    // Recursively build the chain
    if (checkpoint.baseCheckpointId && checkpoint.baseCheckpointId !== baseId) {
      const parentDeltas = await this.buildDeltaChain(baseId, checkpoint.baseCheckpointId);
      deltas.push(...parentDeltas);
    }

    // Add the current delta
    if (checkpoint.delta) {
      deltas.push(checkpoint.delta);
    }

    return deltas;
  }

  /**
   * Create restore result from snapshot
   * Can be overridden to provide custom metadata
   */
  protected createRestoreResult(snapshot: TSnapshot): TRestoreResult {
    return {
      snapshot,
    } as TRestoreResult;
  }

  /**
   * Extract snapshot from checkpoint
   */
  protected abstract extractSnapshot(checkpoint: TCheckpoint): TSnapshot;

  /**
   * Check if checkpoint has snapshot
   */
  protected abstract hasSnapshot(checkpoint: TCheckpoint): boolean;

  /**
   * Extract parent ID from checkpoint (for listing)
   */
  protected abstract extractParentId(checkpoint: TCheckpoint): string;

  /**
   * Apply delta to snapshot
   */
  protected abstract applyDelta(snapshot: TSnapshot, delta: TDelta): TSnapshot;
}

/**
 * Create a simple checkpoint loader
 */
export function createCheckpointLoader<
  TCheckpoint extends BaseCheckpoint<unknown, unknown>,
>(options: {
  load: (id: string) => Promise<TCheckpoint | null>;
  list: (parentId: string) => Promise<string[]>;
}): CheckpointLoader<TCheckpoint> {
  return {
    load: options.load,
    list: options.list,
  };
}
