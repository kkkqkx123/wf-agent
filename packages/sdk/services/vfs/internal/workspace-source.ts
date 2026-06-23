/**
 * WorkspaceSource — Host filesystem access with path policy enforcement
 *
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Provides read-write access to the host workspace filesystem, restricted by
 * path policies (readable/writable whitelists). All operations are executed
 * directly on the host filesystem after policy checking — there is no
 * intermediate storage layer.
 *
 * Unlike the original VFS design that used WorkspaceSource as a read-only
 * base layer under a writable delta, this implementation performs all
 * operations directly on the host FS. This guarantees that tools, external
 * processes, and shell commands always see consistent file state.
 */

import * as fs from "node:fs";
import * as fsPromise from "node:fs/promises";
import * as path from "node:path";
import type { VFSEntry, VFSOperations } from "../types.js";

export interface PathPolicy {
  readable?: string[];
  writable?: string[];
}

export class WorkspaceSource implements VFSOperations {
  private workspaceRoot: string;
  private pathPolicy: PathPolicy;

  constructor(workspaceRoot: string, pathPolicy?: PathPolicy) {
    this.workspaceRoot = path.resolve(workspaceRoot);
    this.pathPolicy = pathPolicy ?? {
      readable: [this.workspaceRoot],
      writable: [this.workspaceRoot],
    };
  }

  /** Get the workspace root path. */
  getWorkspaceRoot(): string {
    return this.workspaceRoot;
  }

  // =========================================================================
  // Read operations
  // =========================================================================

  async stat(hostPath: string): Promise<VFSEntry | null> {
    if (!this.isReadAllowed(hostPath)) return null;

    try {
      const stats = await fsPromise.stat(hostPath);
      return this.toVFSEntry(hostPath, stats);
    } catch {
      return null;
    }
  }

  async readFile(hostPath: string): Promise<Buffer | null> {
    if (!this.isReadAllowed(hostPath)) return null;

    try {
      return await fsPromise.readFile(hostPath);
    } catch {
      return null;
    }
  }

  async readdir(hostPath: string): Promise<VFSEntry[]> {
    if (!this.isReadAllowed(hostPath)) return [];

    try {
      const entries = await fsPromise.readdir(hostPath, { withFileTypes: true });
      const result: VFSEntry[] = [];

      for (const entry of entries) {
        const fullPath = path.join(hostPath, entry.name);
        if (!this.isReadAllowed(fullPath)) continue;

        try {
          const stats = await fsPromise.stat(fullPath);
          result.push(this.toVFSEntry(fullPath, stats));
        } catch {
          // Skip entries that can't be stat'd
        }
      }

      return result;
    } catch {
      return [];
    }
  }

  async exists(hostPath: string): Promise<boolean> {
    if (!this.isReadAllowed(hostPath)) return false;

    try {
      await fsPromise.access(hostPath, fs.constants.F_OK);
      return true;
    } catch {
      return false;
    }
  }

  // =========================================================================
  // Write operations — directly on host filesystem with policy check
  // =========================================================================

  async writeFile(p: string, data: Buffer): Promise<void> {
    const hostPath = path.resolve(p);
    if (!this.isWriteAllowed(hostPath)) {
      throw new Error(`Write denied: ${p} is not in the writable paths`);
    }
    await fsPromise.mkdir(path.dirname(hostPath), { recursive: true });
    await fsPromise.writeFile(hostPath, data);
  }

  async remove(p: string): Promise<void> {
    const hostPath = path.resolve(p);
    if (!this.isWriteAllowed(hostPath)) {
      throw new Error(`Remove denied: ${p} is not in the writable paths`);
    }
    await fsPromise.rm(hostPath, { recursive: true, force: true });
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    const hostOld = path.resolve(oldPath);
    const hostNew = path.resolve(newPath);
    if (!this.isWriteAllowed(hostOld)) {
      throw new Error(`Rename denied: ${oldPath} is not in the writable paths`);
    }
    if (!this.isWriteAllowed(hostNew)) {
      throw new Error(`Rename denied: ${newPath} is not in the writable paths`);
    }
    await fsPromise.mkdir(path.dirname(hostNew), { recursive: true });
    await fsPromise.rename(hostOld, hostNew);
  }

  async mkdir(p: string): Promise<void> {
    const hostPath = path.resolve(p);
    if (!this.isWriteAllowed(hostPath)) {
      throw new Error(`Mkdir denied: ${p} is not in the writable paths`);
    }
    await fsPromise.mkdir(hostPath, { recursive: true });
  }

  async rmdir(p: string): Promise<void> {
    const hostPath = path.resolve(p);
    if (!this.isWriteAllowed(hostPath)) {
      throw new Error(`Rmdir denied: ${p} is not in the writable paths`);
    }
    await fsPromise.rmdir(hostPath);
  }

  // =========================================================================
  // Path operations
  // =========================================================================

  /**
   * Check if a path is allowed by the path policy (for read operations).
   *
   * Allow if:
   *   - No readable paths are configured (allow all)
   *   - Path is within one of the readable prefixes
   */
  isReadAllowed(hostPath: string): boolean {
    const resolved = path.resolve(hostPath);

    if (!this.pathPolicy.readable || this.pathPolicy.readable.length === 0) {
      return true;
    }

    return this.pathPolicy.readable.some(allowed => {
      const allowedPath = path.resolve(allowed);
      return resolved === allowedPath || resolved.startsWith(allowedPath + path.sep);
    });
  }

  /**
   * Check if a path is allowed for write operations.
   *
   * Falls back to readable policy if writable is not configured.
   */
  isWriteAllowed(hostPath: string): boolean {
    const resolved = path.resolve(hostPath);
    const writable = this.pathPolicy.writable ?? this.pathPolicy.readable;

    if (!writable || writable.length === 0) {
      return true;
    }

    return writable.some(allowed => {
      const allowedPath = path.resolve(allowed);
      return resolved === allowedPath || resolved.startsWith(allowedPath + path.sep);
    });
  }

  isAllowed(p: string): boolean {
    return this.isReadAllowed(p);
  }

  translatePath(p: string): string {
    return path.resolve(p);
  }

  // =========================================================================
  // Internal helpers
  // =========================================================================

  private toVFSEntry(hostPath: string, stats: fs.Stats): VFSEntry {
    return {
      name: path.basename(hostPath),
      path: this.toVFSPath(hostPath),
      type: stats.isDirectory() ? "directory" : "file",
      size: stats.size,
      mode: stats.mode,
      createdAt: stats.birthtimeMs,
      modifiedAt: stats.mtimeMs,
    };
  }

  /**
   * Convert a host path to a VFS path (relative to workspace root).
   */
  private toVFSPath(hostPath: string): string {
    const resolved = path.resolve(hostPath);
    if (resolved.startsWith(this.workspaceRoot)) {
      const relative = path.relative(this.workspaceRoot, resolved);
      return `/${relative.replace(/\\/g, "/")}`;
    }
    return `/${resolved.replace(/\\/g, "/")}`;
  }
}
