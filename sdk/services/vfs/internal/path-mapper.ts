/**
 * PathMapper — Maps sandbox virtual paths to host directories
 *
 * Architecture reference: docs/infra/sandbox/improvement-plan.md §Phase 1
 * Reference implementation: ref/agentfs/sandbox/src/vfs/bind.rs
 *
 * Translates paths under a sandbox prefix to a host directory. Unlike SandboxVFS,
 * PathMapper is a transparent passthrough — it doesn't create a delta layer.
 */

import * as path from "node:path";
import * as fsPromise from "node:fs/promises";
import * as fs from "node:fs";
import { basename } from "node:path";
import type { VFSEntry, VFSOperations } from "../types.js";

export class PathMapper implements VFSOperations {
  private hostRoot: string;
  private sandboxRoot: string;

  /**
   * @param hostRoot - Real directory on the host filesystem
   * @param sandboxRoot - Virtual path as seen by the sandboxed process
   */
  constructor(hostRoot: string, sandboxRoot: string) {
    this.hostRoot = path.resolve(hostRoot);
    this.sandboxRoot = this.normalize(sandboxRoot);
  }

  getHostRoot(): string {
    return this.hostRoot;
  }

  getSandboxRoot(): string {
    return this.sandboxRoot;
  }

  /**
   * Translate a sandbox virtual path to a host path.
   */
  toHostPath(sandboxPath: string): string {
    const normalized = this.normalize(sandboxPath);

    if (normalized === this.sandboxRoot) {
      return this.hostRoot;
    }

    if (normalized.startsWith(this.sandboxRoot + "/")) {
      const relative = normalized.slice(this.sandboxRoot.length);
      return path.join(this.hostRoot, relative.replace(/^\//, ""));
    }

    // Path not under our mount — return as-is
    return path.resolve(sandboxPath);
  }

  /**
   * Translate a host path back to a sandbox virtual path.
   */
  toSandboxPath(hostPath: string): string {
    const resolved = path.resolve(hostPath);

    if (resolved === this.hostRoot) {
      return this.sandboxRoot;
    }

    if (resolved.startsWith(this.hostRoot + path.sep)) {
      const relative = path.relative(this.hostRoot, resolved);
      return this.sandboxRoot + "/" + relative.replace(/\\/g, "/");
    }

    return "/" + resolved.replace(/\\/g, "/");
  }

  // =========================================================================
  // VFSOperations implementation
  // =========================================================================

  async stat(p: string): Promise<VFSEntry | null> {
    const hostPath = this.toHostPath(p);
    try {
      const stats = await fsPromise.stat(hostPath);
      return {
        name: basename(hostPath),
        path: this.toSandboxPath(hostPath),
        type: stats.isDirectory() ? "directory" : "file",
        size: stats.size,
        mode: stats.mode,
        createdAt: stats.birthtimeMs,
        modifiedAt: stats.mtimeMs,
      };
    } catch {
      return null;
    }
  }

  async readFile(p: string): Promise<Buffer | null> {
    const hostPath = this.toHostPath(p);
    try {
      return await fsPromise.readFile(hostPath);
    } catch {
      return null;
    }
  }

  async writeFile(p: string, data: Buffer): Promise<void> {
    const hostPath = this.toHostPath(p);
    await fsPromise.mkdir(path.dirname(hostPath), { recursive: true });
    await fsPromise.writeFile(hostPath, data);
  }

  async remove(p: string): Promise<void> {
    const hostPath = this.toHostPath(p);
    try {
      await fsPromise.unlink(hostPath);
    } catch {
      // If it's a directory, try rm
      try {
        await fsPromise.rm(hostPath, { recursive: true, force: true });
      } catch {
        // Ignore
      }
    }
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    const hostOld = this.toHostPath(oldPath);
    const hostNew = this.toHostPath(newPath);
    await fsPromise.mkdir(path.dirname(hostNew), { recursive: true });
    await fsPromise.rename(hostOld, hostNew);
  }

  async readdir(p: string): Promise<VFSEntry[]> {
    const hostPath = this.toHostPath(p);
    try {
      const entries = await fsPromise.readdir(hostPath, { withFileTypes: true });
      const result: VFSEntry[] = [];

      for (const entry of entries) {
        const fullPath = path.join(hostPath, entry.name);
        try {
          const stats = await fsPromise.stat(fullPath);
          result.push({
            name: entry.name,
            path: this.toSandboxPath(fullPath),
            type: stats.isDirectory() ? "directory" : "file",
            size: stats.size,
            mode: stats.mode,
            createdAt: stats.birthtimeMs,
            modifiedAt: stats.mtimeMs,
          });
        } catch {
          // Skip inaccessible entries
        }
      }

      return result;
    } catch {
      return [];
    }
  }

  async mkdir(p: string): Promise<void> {
    const hostPath = this.toHostPath(p);
    await fsPromise.mkdir(hostPath, { recursive: true });
  }

  async rmdir(p: string): Promise<void> {
    const hostPath = this.toHostPath(p);
    await fsPromise.rmdir(hostPath);
  }

  async exists(p: string): Promise<boolean> {
    const hostPath = this.toHostPath(p);
    try {
      await fsPromise.access(hostPath, fs.constants.F_OK);
      return true;
    } catch {
      return false;
    }
  }

  isAllowed(_p: string): boolean {
    return true;
  }

  translatePath(p: string): string {
    return this.toHostPath(p);
  }

  private normalize(p: string): string {
    let normalized = p.replace(/\\/g, "/").replace(/\/+/g, "/");
    if (normalized.length > 1 && normalized.endsWith("/")) {
      normalized = normalized.slice(0, -1);
    }
    return normalized.startsWith("/") ? normalized : "/" + normalized;
  }
}
