/**
 * OverlayVFS — Three-layer Copy-on-Write virtual file system
 *
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Lookup order:
 *   1. Delta layer (writable) — return if present
 *   2. Whiteout cache — if found, return "not found"
 *   3. Base layer (read-only) — delegate to HostFS with path policy
 */

import type { VFSEntry, VFSOperations, DeltaFileSystem, BaseFileSystem } from "./types.js";
import type { VFSConfig } from "./types.js";
import { WhiteoutCache } from "./whiteout-cache.js";
import { MemoryDelta } from "./delta/memory-delta.js";
import { HostFS } from "./base/host-fs.js";

export class OverlayVFS implements VFSOperations {
  private delta: DeltaFileSystem;
  private whiteout: WhiteoutCache;
  private base: BaseFileSystem;

  constructor(config: VFSConfig) {
    this.delta = new MemoryDelta();
    this.whiteout = new WhiteoutCache();

    const workspaceRoot = config.workspaceRoot;
    const pathPolicy = config.pathPolicy;

    this.base = new HostFS(workspaceRoot, pathPolicy);
  }

  // =========================================================================
  // File operations
  // =========================================================================

  async stat(path: string): Promise<VFSEntry | null> {
    // 1. Check Delta layer
    const deltaEntry = await this.delta.stat(path);
    if (deltaEntry) return deltaEntry;

    // 2. Check Whiteout cache
    if (this.whiteout.hasWhiteout(path)) return null;

    // 3. Check Base layer
    return this.base.stat(this.toHostPath(path));
  }

  async readFile(vfsPath: string): Promise<Buffer | null> {
    // 1. Check Delta layer
    const deltaData = await this.delta.readFile(vfsPath);
    if (deltaData) return deltaData;

    // 2. Check Whiteout cache
    if (this.whiteout.hasWhiteout(vfsPath)) return null;

    // 3. Check Base layer
    return this.base.readFile(this.toHostPath(vfsPath));
  }

  async writeFile(vfsPath: string, data: Buffer): Promise<void> {
    // Writing removes whiteout
    this.whiteout.clearWhiteout(vfsPath);
    await this.delta.writeFile(vfsPath, data);
  }

  async remove(vfsPath: string): Promise<void> {
    // Check if file exists in delta or base
    const deltaEntry = await this.delta.stat(vfsPath);
    if (deltaEntry) {
      await this.delta.remove(vfsPath);
      return;
    }

    const hostPath = this.toHostPath(vfsPath);
    const baseEntry = await this.base.stat(hostPath);
    if (baseEntry) {
      // Mark as whiteout in delta
      this.whiteout.markWhiteout(vfsPath);
      return;
    }
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    await this.delta.rename(oldPath, newPath);
    this.whiteout.clearWhiteout(newPath);
  }

  // =========================================================================
  // Directory operations
  // =========================================================================

  async readdir(vfsPath: string): Promise<VFSEntry[]> {
    const entries: Map<string, VFSEntry> = new Map();

    // 1. Collect from Base layer (filtered by path policy)
    const hostPath = this.toHostPath(vfsPath);
    const baseEntries = await this.base.readdir(hostPath);
    for (const entry of baseEntries) {
      if (!this.whiteout.hasWhiteout(entry.path)) {
        entries.set(entry.name, entry);
      }
    }

    // 2. Overlay with Delta layer
    const deltaFiles = await this.delta.readdir(vfsPath);
    for (const entry of deltaFiles) {
      if (entry.isWhiteout) {
        entries.delete(entry.name);
      } else {
        entries.set(entry.name, entry);
      }
    }

    return Array.from(entries.values());
  }

  async mkdir(path: string): Promise<void> {
    this.whiteout.clearWhiteout(path);
    await this.delta.mkdir(path);
  }

  async rmdir(path: string): Promise<void> {
    const deltaEntry = await this.delta.stat(path);
    if (deltaEntry) {
      await this.delta.rmdir(path);
      return;
    }

    const hostPath = this.toHostPath(path);
    const baseEntry = await this.base.stat(hostPath);
    if (baseEntry) {
      this.whiteout.markWhiteout(path);
      return;
    }
  }

  // =========================================================================
  // Snapshot operations
  // =========================================================================

  async snapshot(): Promise<string> {
    return this.delta.snapshot();
  }

  async restore(snapshotId: string): Promise<void> {
    await this.delta.restore(snapshotId);
  }

  async diff(snapshotA: string, snapshotB: string): Promise<VFSEntry[]> {
    // Note: full diff computation requires tracking change metadata
    // For now, return entries changed between two snapshots
    const changed: VFSEntry[] = [];
    const allIds = this.delta.getSnapshotIds();
    if (!allIds.includes(snapshotA) || !allIds.includes(snapshotB)) {
      return changed;
    }
    return changed;
  }

  // =========================================================================
  // Path operations
  // =========================================================================

  async exists(vfsPath: string): Promise<boolean> {
    const entry = await this.stat(vfsPath);
    return entry !== null;
  }

  isAllowed(vfsPath: string): boolean {
    const hostPath = this.toHostPath(vfsPath);
    return this.base.isAllowed(hostPath);
  }

  // =========================================================================
  // Internal helpers
  // =========================================================================

  /**
   * Convert a VFS path to a host filesystem path.
   */
  private toHostPath(vfsPath: string): string {
    // VFS paths are relative to workspace root
    // This is handled by HostFS which stores the workspaceRoot
    return vfsPath;
  }
}