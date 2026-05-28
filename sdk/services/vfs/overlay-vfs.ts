/**
 * OverlayVFS — Two-layer overlay virtual file system
 *
 * Architecture:
 *   1. Delta layer (SQLite-backed, writable) — files created/modified by sandbox
 *   2. Base layer (HostFS, read-only) — access to host workspace with path policy
 *
 * Lookup order: Delta → Base
 *
 * VFS is for sandbox access control during script execution.
 * File persistence is handled by SqliteDelta; snapshot/checkpoint
 * integration is managed by CheckpointCoordinator at the workflow level.
 */

import * as path from "node:path";
import type { VFSEntry, VFSOperations, DeltaFileSystem, BaseFileSystem } from "./types.js";
import type { VFSProvider } from "@wf-agent/types";
import type { VFSConfig } from "./types.js";
import { SqliteDelta } from "./delta/sqlite-delta.js";
import { HostFS } from "./base/host-fs.js";

export class OverlayVFS implements VFSOperations, VFSProvider {
  private delta: DeltaFileSystem;
  private base: BaseFileSystem;
  private workspaceRoot: string;

  /**
   * Track base-file deletions so read/stat don't fall through to base.
   * This is a minimal replacement for the removed WhiteoutCache trie.
   */
  private deletedBaseFiles = new Set<string>();

  constructor(config: VFSConfig) {
    this.workspaceRoot = config.workspaceRoot;

    const dbPath = config.dbPath ?? ":memory:";
    this.delta = new SqliteDelta({ dbPath });

    this.base = new HostFS(config.workspaceRoot, config.pathPolicy);
  }

  // =========================================================================
  // File operations
  // =========================================================================

  async stat(path: string): Promise<VFSEntry | null> {
    const normalizedPath = this.normalize(path);

    if (this.deletedBaseFiles.has(normalizedPath)) {
      return null;
    }

    const deltaEntry = await this.delta.stat(normalizedPath);
    if (deltaEntry) return deltaEntry;

    return this.base.stat(this.toHostPath(normalizedPath));
  }

  async readFile(vfsPath: string): Promise<Buffer | null> {
    const normalizedPath = this.normalize(vfsPath);

    if (this.deletedBaseFiles.has(normalizedPath)) {
      return null;
    }

    const deltaData = await this.delta.readFile(normalizedPath);
    if (deltaData) return deltaData;

    return this.base.readFile(this.toHostPath(normalizedPath));
  }

  async writeFile(vfsPath: string, data: Buffer): Promise<void> {
    const normalizedPath = this.normalize(vfsPath);
    this.deletedBaseFiles.delete(normalizedPath);
    await this.delta.writeFile(normalizedPath, data);
  }

  async remove(vfsPath: string): Promise<void> {
    const normalizedPath = this.normalize(vfsPath);

    const deltaEntry = await this.delta.stat(normalizedPath);
    if (deltaEntry) {
      await this.delta.remove(normalizedPath);
      return;
    }

    const hostPath = this.toHostPath(normalizedPath);
    const baseEntry = await this.base.stat(hostPath);
    if (baseEntry) {
      this.deletedBaseFiles.add(normalizedPath);
    }
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    const oldNormalized = this.normalize(oldPath);
    const newNormalized = this.normalize(newPath);

    this.deletedBaseFiles.delete(oldNormalized);
    this.deletedBaseFiles.delete(newNormalized);

    await this.delta.rename(oldNormalized, newNormalized);
  }

  // =========================================================================
  // Directory operations
  // =========================================================================

  async readdir(vfsPath: string): Promise<VFSEntry[]> {
    const normalizedPath = this.normalize(vfsPath);
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

  async mkdir(vfsPath: string): Promise<void> {
    const normalizedPath = this.normalize(vfsPath);
    this.deletedBaseFiles.delete(normalizedPath);
    await this.delta.mkdir(normalizedPath);
  }

  async rmdir(vfsPath: string): Promise<void> {
    const normalizedPath = this.normalize(vfsPath);

    const deltaEntry = await this.delta.stat(normalizedPath);
    if (deltaEntry) {
      await this.delta.rmdir(normalizedPath);
      return;
    }

    const hostPath = this.toHostPath(normalizedPath);
    const baseEntry = await this.base.stat(hostPath);
    if (baseEntry) {
      this.deletedBaseFiles.add(normalizedPath);
    }
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