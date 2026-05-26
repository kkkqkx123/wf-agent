/**
 * MemoryDelta — In-memory delta layer implementation
 *
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Stores all modifications in memory as a Map. Supports snapshots
 * for checkpoint integration. Changes are discarded on process exit.
 */

import { randomUUID } from "node:crypto";
import { basename } from "node:path";
import type { VFSEntry, DeltaFileSystem } from "../types.js";

type FileRecord = { data: Buffer; entry: VFSEntry };

export class MemoryDelta implements DeltaFileSystem {
  private files = new Map<string, FileRecord>();
  private directories = new Map<string, VFSEntry>();
  private snapshots = new Map<string, Map<string, FileRecord>>();

  async stat(path: string): Promise<VFSEntry | null> {
    const normalized = this.normalize(path);

    // Check files
    const file = this.files.get(normalized);
    if (file) return file.entry;

    // Check directories
    const dir = this.directories.get(normalized);
    if (dir) return dir;

    return null;
  }

  async readFile(path: string): Promise<Buffer | null> {
    const normalized = this.normalize(path);
    const file = this.files.get(normalized);
    return file?.data ?? null;
  }

  async writeFile(path: string, data: Buffer): Promise<void> {
    const normalized = this.normalize(path);
    const now = Date.now();

    this.files.set(normalized, {
      data,
      entry: {
        name: basename(normalized),
        path: normalized,
        type: "file",
        size: data.length,
        mode: 0o644,
        createdAt: now,
        modifiedAt: now,
        isWhiteout: false,
      },
    });

    // Ensure parent directory exists
    await this.ensureParentDir(normalized);
  }

  async remove(path: string): Promise<void> {
    const normalized = this.normalize(path);
    this.files.delete(normalized);

    // Remove all children if directory
    const prefix = `${normalized}/`;
    for (const key of this.files.keys()) {
      if (key.startsWith(prefix)) {
        this.files.delete(key);
      }
    }
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    const oldNormalized = this.normalize(oldPath);
    const newNormalized = this.normalize(newPath);

    const file = this.files.get(oldNormalized);
    if (!file) return;

    const now = Date.now();
    const renamedRecord: FileRecord = {
      data: file.data,
      entry: {
        ...file.entry,
        name: basename(newNormalized),
        path: newNormalized,
        modifiedAt: now,
      },
    };

    this.files.delete(oldNormalized);
    this.files.set(newNormalized, renamedRecord);
    await this.ensureParentDir(newNormalized);
  }

  async readdir(path: string): Promise<VFSEntry[]> {
    const normalized = this.normalize(path);
    const prefix = normalized === "/" ? "" : `${normalized}/`;
    const entries: VFSEntry[] = [];

    for (const [key, file] of this.files) {
      if (key.startsWith(prefix) && !key.slice(prefix.length).includes("/")) {
        entries.push(file.entry);
      }
    }

    // Add directories that have child files
    const seenDirs = new Set<string>();
    for (const key of this.files.keys()) {
      if (key.startsWith(prefix)) {
        const subPath = key.slice(prefix.length);
        const firstSlash = subPath.indexOf("/");
        if (firstSlash > 0) {
          const dirName = subPath.slice(0, firstSlash);
          if (!seenDirs.has(dirName)) {
            seenDirs.add(dirName);
            const dir = this.directories.get(`${prefix}${dirName}`);
            if (dir) {
              entries.push(dir);
            }
          }
        }
      }
    }

    return entries;
  }

  async mkdir(path: string): Promise<void> {
    const normalized = this.normalize(path);
    const now = Date.now();

    this.directories.set(normalized, {
      name: basename(normalized === "/" ? "/" : normalized),
      path: normalized,
      type: "directory",
      size: 0,
      mode: 0o755,
      createdAt: now,
      modifiedAt: now,
      isWhiteout: false,
    });

    await this.ensureParentDir(normalized);
  }

  async rmdir(path: string): Promise<void> {
    const normalized = this.normalize(path);
    this.directories.delete(normalized);

    // Remove all children
    const prefix = `${normalized}/`;
    for (const key of this.files.keys()) {
      if (key.startsWith(prefix)) {
        this.files.delete(key);
      }
    }
  }

  async snapshot(): Promise<string> {
    const id = randomUUID();
    const snapshot = new Map<string, FileRecord>();
    for (const [key, value] of this.files) {
      snapshot.set(key, {
        data: Buffer.from(value.data),
        entry: { ...value.entry },
      });
    }
    this.snapshots.set(id, snapshot);
    return id;
  }

  async restore(snapshotId: string): Promise<void> {
    const snapshot = this.snapshots.get(snapshotId);
    if (!snapshot) {
      throw new Error(`Snapshot not found: ${snapshotId}`);
    }
    this.files = new Map<string, FileRecord>();
    for (const [key, value] of snapshot) {
      this.files.set(key, {
        data: Buffer.from(value.data),
        entry: { ...value.entry },
      });
    }
  }

  getSnapshotIds(): string[] {
    return Array.from(this.snapshots.keys());
  }

  hasPendingChanges(): boolean {
    return this.files.size > 0 || this.directories.size > 0;
  }

  /**
   * Normalize a path to POSIX-style.
   */
  private normalize(path: string): string {
    const normalized = path.replace(/\\/g, "/").replace(/\/+/g, "/");

    // Remove trailing slash (except root)
    if (normalized.length > 1 && normalized.endsWith("/")) {
      return normalized.slice(0, -1);
    }

    // Ensure leading slash
    return normalized.startsWith("/") ? normalized : `/${normalized}`;
  }

  /**
   * Ensure parent directory entries exist for a given path.
   */
  private async ensureParentDir(path: string): Promise<void> {
    const parentDir = path.substring(0, path.lastIndexOf("/"));
    if (!parentDir || parentDir === "") return;

    const now = Date.now();
    if (!this.directories.has(parentDir)) {
      this.directories.set(parentDir, {
        name: basename(parentDir),
        path: parentDir,
        type: "directory",
        size: 0,
        mode: 0o755,
        createdAt: now,
        modifiedAt: now,
        isWhiteout: false,
      });
    }

    // Recursively ensure parent's parent
    await this.ensureParentDir(parentDir);
  }
}