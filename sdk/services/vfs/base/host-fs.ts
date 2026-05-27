/**
 * HostFS — Read-only base layer with path policy
 *
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Provides read-only access to the host filesystem, restricted by
 * a path whitelist (pathPolicy). Only paths matching the readable
 * whitelist can be accessed.
 */

import * as fs from "node:fs";
import * as fsPromise from "node:fs/promises";
import * as path from "node:path";
import type { VFSEntry, BaseFileSystem } from "../types.js";

export interface PathPolicy {
  readable?: string[];
}

export class HostFS implements BaseFileSystem {
  private workspaceRoot: string;
  private pathPolicy: PathPolicy;

  constructor(workspaceRoot: string, pathPolicy?: PathPolicy) {
    this.workspaceRoot = path.resolve(workspaceRoot);
    this.pathPolicy = pathPolicy ?? { readable: [this.workspaceRoot] };
  }

  async stat(hostPath: string): Promise<VFSEntry | null> {
    if (!this.isAllowed(hostPath)) return null;

    try {
      const stats = await fsPromise.stat(hostPath);
      return this.toVFSEntry(hostPath, stats);
    } catch {
      return null;
    }
  }

  async readFile(hostPath: string): Promise<Buffer | null> {
    if (!this.isAllowed(hostPath)) return null;

    try {
      return await fsPromise.readFile(hostPath);
    } catch {
      return null;
    }
  }

  async readdir(hostPath: string): Promise<VFSEntry[]> {
    if (!this.isAllowed(hostPath)) return [];

    try {
      const entries = await fsPromise.readdir(hostPath, { withFileTypes: true });
      const result: VFSEntry[] = [];

      for (const entry of entries) {
        const fullPath = path.join(hostPath, entry.name);
        if (!this.isAllowed(fullPath)) continue;

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
    if (!this.isAllowed(hostPath)) return false;

    try {
      await fsPromise.access(hostPath, fs.constants.F_OK);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Check if a path is allowed by the path policy.
   *
   * Allow if:
   *   - No readable paths are configured (allow all)
   *   - Path is within one of the readable prefixes
   */
  isAllowed(hostPath: string): boolean {
    const resolved = path.resolve(hostPath);

    if (!this.pathPolicy.readable || this.pathPolicy.readable.length === 0) {
      return true;
    }

    return this.pathPolicy.readable.some((allowed) => {
      const allowedPath = path.resolve(allowed);
      return resolved === allowedPath || resolved.startsWith(allowedPath + path.sep);
    });
  }

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