/**
 * Generic Delta Restorer
 *
 * Provides generic restoration logic for snapshots with delta support.
 * Used by both Graph and Agent modules.
 */

import type { SnapshotBase } from "@wf-agent/types";

/**
 * Snapshot loader function type
 */
export type SnapshotLoader<TSnapshot extends SnapshotBase> = (
  id: string,
) => Promise<TSnapshot | null>;

/**
 * Snapshot lister function type
 */
export type SnapshotLister = (parentId: string) => Promise<string[]>;

/**
 * Restoration result
 */
export interface RestorationResult<TSnapshot extends SnapshotBase> {
  /** The restored full snapshot */
  snapshot: TSnapshot;
  /** Number of deltas applied */
  deltasApplied: number;
  /** Chain of checkpoint IDs used for restoration */
  restorationChain: string[];
}

/**
 * Generic Delta Restorer
 *
 * @template TSnapshot The snapshot type
 */
export class DeltaRestorer<TSnapshot extends SnapshotBase> {
  constructor(
    protected readonly loadSnapshot: SnapshotLoader<TSnapshot>,
    protected readonly listSnapshots: SnapshotLister,
  ) {}

  /**
   * Restore a full snapshot from a potentially delta-based checkpoint
   *
   * @param snapshotId The snapshot ID to restore from
   * @returns Restoration result with full snapshot
   */
  async restore(snapshotId: string): Promise<RestorationResult<TSnapshot>> {
    const snapshot = await this.loadSnapshot(snapshotId);

    if (!snapshot) {
      throw new Error(`Snapshot not found: ${snapshotId}`);
    }

    if (!this.isDeltaSnapshot(snapshot)) {
      return {
        snapshot,
        deltasApplied: 0,
        restorationChain: [snapshotId],
      };
    }

    return this.restoreFromDelta(snapshot, snapshotId);
  }

  /**
   * Check if a snapshot is a delta snapshot
   */
  protected isDeltaSnapshot(snapshot: TSnapshot): boolean {
    return (
      (snapshot as TSnapshot & { type?: string }).type === "DELTA" ||
      (snapshot as TSnapshot & { delta?: unknown }).delta !== undefined
    );
  }

  /**
   * Restore from a delta snapshot
   */
  protected async restoreFromDelta(
    deltaSnapshot: TSnapshot,
    deltaSnapshotId: string,
  ): Promise<RestorationResult<TSnapshot>> {
    const typedSnapshot = deltaSnapshot as TSnapshot & {
      baseSnapshotId?: string;
      delta?: Partial<TSnapshot>;
    };

    if (!typedSnapshot.baseSnapshotId) {
      throw new Error("Delta snapshot missing baseSnapshotId");
    }

    const baseSnapshot = await this.loadSnapshot(typedSnapshot.baseSnapshotId);

    if (!baseSnapshot) {
      throw new Error(`Base snapshot not found: ${typedSnapshot.baseSnapshotId}`);
    }

    const baseResult = await this.restore(typedSnapshot.baseSnapshotId);

    const restoredSnapshot = this.applyDelta(baseResult.snapshot, typedSnapshot.delta ?? {});

    return {
      snapshot: restoredSnapshot,
      deltasApplied: baseResult.deltasApplied + 1,
      restorationChain: [...baseResult.restorationChain, deltaSnapshotId],
    };
  }

  /**
   * Apply a delta to a base snapshot
   *
   * @param base The base snapshot
   * @param delta The delta to apply
   * @returns The merged snapshot
   */
  protected applyDelta(base: TSnapshot, delta: Partial<TSnapshot>): TSnapshot {
    return {
      ...base,
      ...delta,
      _version: base._version,
      _timestamp: Date.now(),
    } as TSnapshot;
  }

  /**
   * Apply multiple deltas in sequence
   *
   * @param base The base snapshot
   * @param deltas Array of deltas to apply in order
   * @returns The final merged snapshot
   */
  applyDeltas(base: TSnapshot, deltas: Partial<TSnapshot>[]): TSnapshot {
    let result = base;
    for (const delta of deltas) {
      result = this.applyDelta(result, delta);
    }
    return result;
  }
}
