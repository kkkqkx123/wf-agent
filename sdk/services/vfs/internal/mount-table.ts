/**
 * MountTable — Multi-mount-point VFS resolver with longest-prefix matching
 *
 * Architecture reference: docs/infra/sandbox/improvement-plan.md §Phase 1
 * Reference implementation: ref/agentfs/sandbox/src/vfs/mount.rs
 *
 * Supports multiple VFS mounts, resolving paths via longest-prefix matching.
 * This enables transparent path rewriting, e.g.:
 *   /agent/special/file  →  PathMapper  →  /tmp/special/file
 *   /agent/normal/file   →  PathMapper  →  /tmp/agent/normal/file
 */

import type { VFSOperations } from "../types.js";

/**
 * A mount point entry.
 * Maps a sandbox virtual path to a VFS implementation.
 */
export interface MountPoint {
  /** Virtual path as seen by the sandboxed process (e.g., "/agent") */
  sandboxPath: string;
  /** The VFS implementation for this mount point */
  vfs: VFSOperations;
}

/**
 * MountTable manages multiple VFS mount points with longest-prefix matching.
 *
 * - Mount points are sorted by path depth (deepest first)
 * - resolve() finds the best match using longest-prefix
 */
export class MountTable {
  private mounts: MountPoint[] = [];

  /**
   * Add a new mount point.
   * Mounts are automatically sorted by path depth (deepest first)
   * to ensure correct longest-prefix matching.
   */
  addMount(sandboxPath: string, vfs: VFSOperations): void {
    // Normalize: ensure starts with /, no trailing /
    const normalized = this.normalize(sandboxPath);
    this.mounts.push({ sandboxPath: normalized, vfs });
    this.mounts.sort(
      (a, b) => b.sandboxPath.split("/").length - a.sandboxPath.split("/").length,
    );
  }

  /**
   * Resolve a path to the best-matching VFS and translated path.
   *
   * Uses longest-prefix matching: among all mount points whose sandboxPath
   * is a prefix of the given path, the one with the deepest path wins.
   *
   * Returns null if no mount point matches.
   */
  resolve(path: string): { vfs: VFSOperations; translatedPath: string } | null {
    const normalized = this.normalize(path);

    for (const mount of this.mounts) {
      const resolved = this.tryResolve(mount, normalized);
      if (resolved) return resolved;
    }

    return null;
  }

  /**
   * Get all registered mount points.
   */
  getMounts(): readonly MountPoint[] {
    return this.mounts;
  }

  /**
   * Remove a mount point by sandbox path.
   */
  removeMount(sandboxPath: string): boolean {
    const normalized = this.normalize(sandboxPath);
    const index = this.mounts.findIndex((m) => m.sandboxPath === normalized);
    if (index !== -1) {
      this.mounts.splice(index, 1);
      return true;
    }
    return false;
  }

  /**
   * Clear all mount points.
   */
  clear(): void {
    this.mounts = [];
  }

  /**
   * Try to resolve a path against a single mount point.
   *
   * A mount point matches if:
   *   - path === mount.sandboxPath (exact match), or
   *   - path starts with "mount.sandboxPath/" (prefix match)
   */
  private tryResolve(
    mount: MountPoint,
    path: string,
  ): { vfs: VFSOperations; translatedPath: string } | null {
    const prefix = mount.sandboxPath;

    if (path === prefix) {
      return { vfs: mount.vfs, translatedPath: "/" };
    }

    if (path.startsWith(prefix + "/")) {
      const relative = path.slice(prefix.length);
      return { vfs: mount.vfs, translatedPath: relative.startsWith("/") ? relative : "/" + relative };
    }

    return null;
  }

  /**
   * Normalize a sandbox path:
   * - Ensure starts with /
   * - Remove trailing /
   * - Collapse // to /
   */
  private normalize(p: string): string {
    let normalized = p.replace(/\\/g, "/").replace(/\/+/g, "/");
    if (normalized.length > 1 && normalized.endsWith("/")) {
      normalized = normalized.slice(0, -1);
    }
    return normalized.startsWith("/") ? normalized : "/" + normalized;
  }
}