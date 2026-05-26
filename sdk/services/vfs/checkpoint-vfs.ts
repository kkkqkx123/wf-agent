/**
 * CheckpointAwareVFS — Checkpoint integration wrapper
 *
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Wraps an OverlayVFS to automatically create snapshots on checkpoint
 * creation and restore on checkpoint restore.
 */

import type { OverlayVFS } from "./overlay-vfs.js";

export class CheckpointAwareVFS {
  private vfs: OverlayVFS;
  private checkpointMap = new Map<string, string>();

  constructor(vfs: OverlayVFS) {
    this.vfs = vfs;
  }

  /**
   * Called when a checkpoint is created.
   * Takes a VFS snapshot and stores the mapping.
   */
  async onCheckpointCreate(checkpointId: string): Promise<void> {
    const snapshotId = await this.vfs.snapshot();
    this.checkpointMap.set(checkpointId, snapshotId);
  }

  /**
   * Called when a checkpoint is restored.
   * Loads the associated VFS snapshot.
   */
  async onCheckpointRestore(checkpointId: string): Promise<void> {
    const snapshotId = this.checkpointMap.get(checkpointId);
    if (snapshotId) {
      await this.vfs.restore(snapshotId);
    }
  }

  /**
   * Called when a checkpoint is deleted.
   * Removes the mapping (snapshot data may be GC'd).
   */
  async onCheckpointDelete(checkpointId: string): Promise<void> {
    this.checkpointMap.delete(checkpointId);
  }

  /**
   * Get the underlying OverlayVFS instance.
   */
  getVFS(): OverlayVFS {
    return this.vfs;
  }
}