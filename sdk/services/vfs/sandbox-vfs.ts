/**
 * SandboxVFS — Sandbox virtual file system with delta-over-base layering
 *
 * Architecture:
 *   1. Delta layer (DeltaStore, writable) — files created/modified by sandbox
 *   2. Base layer (WorkspaceSource, read-only) — access to host workspace with path policy
 *   3. MountTable — multi-mount longest-prefix path resolution
 *
 * Lookup order: MountTable resolve → Delta → Base
 *
 * SandboxVFS is the top-level VFS entry point for sandbox execution.
 * File checkpoint/restore is managed by CheckpointCoordinator at the workflow level.
 */

import * as path from "node:path";
import * as fs from "node:fs/promises";
import type { VFSEntry, VFSOperations, DeltaFileSystem, Snapshotable } from "./types.js";
import type { VFSConfig, VFSProvider } from "@wf-agent/types";
import { MountTable } from "./internal/mount-table.js";
import { PathMapper } from "./internal/path-mapper.js";
import { DeltaStore } from "./internal/delta-store.js";
import { WorkspaceSource } from "./internal/workspace-source.js";

export class SandboxVFS implements VFSOperations, VFSProvider, Snapshotable {
  private delta: DeltaFileSystem;
  private base: VFSOperations;
  private workspaceRoot: string;
  private mountTable: MountTable;
  private syncToHostFS: boolean;

  /**
   * Track base-file deletions so read/stat don't fall through to base.
   */
  private deletedBaseFiles = new Set<string>();

  constructor(config: VFSConfig) {
    this.workspaceRoot = config.workspaceRoot;
    this.mountTable = new MountTable();
    this.syncToHostFS = config.syncToHostFS ?? false;

    const dbPath = config.dbPath ?? ":memory:";
    this.delta = new DeltaStore({ dbPath });

    this.base = new WorkspaceSource(config.workspaceRoot, config.pathPolicy);

    // Initialize mount points from config
    if (config.mounts) {
      for (const mount of config.mounts) {
        const pathMapper = new PathMapper(mount.hostPath, mount.sandboxPath);
        this.mountTable.addMount(mount.sandboxPath, pathMapper);
      }
    }
  }

  /**
   * Get the MountTable instance for direct access.
   */
  getMountTable(): MountTable {
    return this.mountTable;
  }

  /**
   * Add a path mapper dynamically.
   */
  addPathMapper(sandboxPath: string, hostPath: string): void {
    const pathMapper = new PathMapper(hostPath, sandboxPath);
    this.mountTable.addMount(sandboxPath, pathMapper);
  }

  // =========================================================================
  // File operations — try MountTable first, then fall back to Delta → Base
  // =========================================================================

  async stat(p: string): Promise<VFSEntry | null> {
    // Resolve via MountTable first
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.stat(resolved.translatedPath);
    }

    const normalizedPath = this.normalize(p);

    if (this.deletedBaseFiles.has(normalizedPath)) {
      return null;
    }

    const deltaEntry = await this.delta.stat(normalizedPath);
    if (deltaEntry) return deltaEntry;

    return this.base.stat(this.toHostPath(normalizedPath));
  }

  async readFile(p: string): Promise<Buffer | null> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.readFile(resolved.translatedPath);
    }

    const normalizedPath = this.normalize(p);

    if (this.deletedBaseFiles.has(normalizedPath)) {
      return null;
    }

    const deltaData = await this.delta.readFile(normalizedPath);
    if (deltaData) return deltaData;

    return this.base.readFile(this.toHostPath(normalizedPath));
  }

  async writeFile(p: string, data: Buffer): Promise<void> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.writeFile(resolved.translatedPath, data);
    }

    const normalizedPath = this.normalize(p);
    this.deletedBaseFiles.delete(normalizedPath);
    await this.delta.writeFile(normalizedPath, data);

    // When sync mode is enabled, flush to host filesystem
    if (this.syncToHostFS) {
      const hostPath = this.toHostPath(normalizedPath);
      await fs.mkdir(path.dirname(hostPath), { recursive: true });
      await fs.writeFile(hostPath, data);
    }
  }

  async remove(p: string): Promise<void> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.remove(resolved.translatedPath);
    }

    const normalizedPath = this.normalize(p);

    const deltaEntry = await this.delta.stat(normalizedPath);
    if (deltaEntry) {
      await this.delta.remove(normalizedPath);

      // When sync mode is enabled, also remove from host filesystem
      if (this.syncToHostFS) {
        const hostPath = this.toHostPath(normalizedPath);
        try {
          await fs.rm(hostPath, { recursive: true, force: true });
        } catch {
          // Host file may not exist; that's fine
        }
      }
      return;
    }

    const hostPath = this.toHostPath(normalizedPath);
    const baseEntry = await this.base.stat(hostPath);
    if (baseEntry) {
      this.deletedBaseFiles.add(normalizedPath);

      // When sync mode is enabled, actually remove from host
      if (this.syncToHostFS) {
        try {
          await fs.rm(hostPath, { recursive: true, force: true });
        } catch {
          // Host file may not exist; that's fine
        }
      }
    }
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    const resolvedOld = this.mountTable.resolve(oldPath);
    const resolvedNew = this.mountTable.resolve(newPath);

    // If both are in MountTable, delegate
    if (resolvedOld && resolvedNew && resolvedOld.vfs === resolvedNew.vfs) {
      return resolvedOld.vfs.rename(resolvedOld.translatedPath, resolvedNew.translatedPath);
    }

    const oldNormalized = this.normalize(oldPath);
    const newNormalized = this.normalize(newPath);

    this.deletedBaseFiles.delete(oldNormalized);
    this.deletedBaseFiles.delete(newNormalized);

    await this.delta.rename(oldNormalized, newNormalized);
  }

  // =========================================================================
  // Directory operations
  // =========================================================================

  async readdir(p: string): Promise<VFSEntry[]> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.readdir(resolved.translatedPath);
    }

    const normalizedPath = this.normalize(p);
    const entries: Map<string, VFSEntry> = new Map();

    const hostPath = this.toHostPath(normalizedPath);
    const baseEntries = await this.base.readdir(hostPath);
    for (const entry of baseEntries) {
      if (!this.deletedBaseFiles.has(entry.path)) {
        entries.set(entry.name, entry);
      }
    }

    const deltaFiles = await this.delta.readdir(normalizedPath);
    for (const entry of deltaFiles) {
      entries.set(entry.name, entry);
    }

    return Array.from(entries.values());
  }

  async mkdir(p: string): Promise<void> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.mkdir(resolved.translatedPath);
    }

    const normalizedPath = this.normalize(p);
    this.deletedBaseFiles.delete(normalizedPath);
    await this.delta.mkdir(normalizedPath);

    // When sync mode is enabled, also create on host
    if (this.syncToHostFS) {
      const hostPath = this.toHostPath(normalizedPath);
      try {
        await fs.mkdir(hostPath, { recursive: true });
      } catch {
        // Directory may already exist; that's fine
      }
    }
  }

  async rmdir(p: string): Promise<void> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.rmdir(resolved.translatedPath);
    }

    const normalizedPath = this.normalize(p);

    const deltaEntry = await this.delta.stat(normalizedPath);
    if (deltaEntry) {
      await this.delta.rmdir(normalizedPath);

      // When sync mode is enabled, also remove from host
      if (this.syncToHostFS) {
        const hostPath = this.toHostPath(normalizedPath);
        try {
          await fs.rmdir(hostPath);
        } catch {
          // Host dir may not exist; that's fine
        }
      }
      return;
    }

    const hostPath = this.toHostPath(normalizedPath);
    const baseEntry = await this.base.stat(hostPath);
    if (baseEntry) {
      this.deletedBaseFiles.add(normalizedPath);

      if (this.syncToHostFS) {
        try {
          await fs.rmdir(hostPath);
        } catch {
          // Host dir may not exist; that's fine
        }
      }
    }
  }

  // =========================================================================
  // Path operations
  // =========================================================================

  async exists(p: string): Promise<boolean> {
    const entry = await this.stat(p);
    return entry !== null;
  }

  isAllowed(p: string): boolean {
    const hostPath = this.toHostPath(p);
    return this.base.isAllowed(hostPath);
  }

  translatePath(p: string): string {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.translatePath(resolved.translatedPath);
    }
    return this.normalize(p);
  }

  /**
   * Check whether sync-to-host mode is enabled.
   */
  isSyncToHostEnabled(): boolean {
    return this.syncToHostFS;
  }

  // =========================================================================
  // Snapshot API (Snapshotable) — delegates to delta layer
  // =========================================================================

  async snapshot(): Promise<string> {
    const delta = this.delta as unknown as Snapshotable;
    if (typeof delta.snapshot !== "function") {
      throw new Error("Delta layer does not support snapshot");
    }
    return delta.snapshot();
  }

  async restore(snapshotId: string): Promise<void> {
    const delta = this.delta as unknown as Snapshotable;
    if (typeof delta.restore !== "function") {
      throw new Error("Delta layer does not support restore");
    }
    return delta.restore(snapshotId);
  }

  async listSnapshots(): Promise<Array<{ id: string; createdAt: number }>> {
    const delta = this.delta as unknown as Snapshotable;
    if (typeof delta.listSnapshots !== "function") {
      return [];
    }
    return delta.listSnapshots();
  }

  async deleteSnapshot(snapshotId: string): Promise<void> {
    const delta = this.delta as unknown as Snapshotable;
    if (typeof delta.deleteSnapshot !== "function") {
      throw new Error("Delta layer does not support deleteSnapshot");
    }
    return delta.deleteSnapshot(snapshotId);
  }

  // =========================================================================
  // Internal helpers
  // =========================================================================

  private normalize(vfsPath: string): string {
    const normalized = vfsPath.replace(/\\/g, "/").replace(/\/+/g, "/");
    if (normalized.length > 1 && normalized.endsWith("/")) {
      return normalized.slice(0, -1);
    }
    return normalized.startsWith("/") ? normalized : `/${normalized}`;
  }

  private toHostPath(vfsPath: string): string {
    const normalized = this.normalize(vfsPath);
    const relative = normalized.startsWith("/") ? normalized.slice(1) : normalized;
    return path.join(this.workspaceRoot, relative.replace(/\//g, path.sep));
  }
}