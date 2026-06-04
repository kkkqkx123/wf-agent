/**
 * SandboxVFS — Policy-enforcing VFS proxy backed by the host filesystem
 *
 * Architecture: VFS as policy enforcement proxy, NOT a storage layer.
 *
 * SandboxVFS focuses on write-path policy enforcement:
 *   - Write operations (writeFile, mkdir, remove, rename, rmdir) are checked
 *     against the configured path policy before execution.
 *   - Read operations (readFile, stat, exists, readdir) are pass-through to
 *     the host filesystem — no policy check, no overhead.
 *
 * In the tool chain, SandboxVFS is NOT used directly by tools. Instead, tools
 * use WriteGuardVFS which combines HostFSAdapter (reads → direct fs) with
 * SandboxVFS (writes → policy check → fs). This ensures read operations have
 * zero overhead while write operations are always guarded.
 *
 * Unlike a traditional VFS that stores data internally, SandboxVFS is a
 * policy enforcement layer: every write operation is checked against the
 * configured path policy, and if allowed, executed directly on the host
 * filesystem. There is no intermediate delta/overlay layer — all operations
 * are synchronous with the host.
 *
 * This design ensures that:
 *   1. Tools reading files always see the real host filesystem state (no VFS
 *      overhead on the critical read path).
 *   2. Write operations are safely intercepted — dangerous writes (e.g.,
 *      modifying system config files, rm -rf outside workspace) are blocked
 *      with a clear error prior to execution.
 *   3. External processes (compilation, shell commands) never encounter stale
 *      data — because there is no "hidden" VFS copy to go stale.
 *   4. Policy violations are caught before any filesystem mutation occurs.
 *
 * Historical note: Earlier versions of this class used a delta-over-base
 * layering (writable delta over read-only WorkspaceSource), with a
 * syncToHostFS flag to push delta writes to the host. This created dual-state
 * inconsistency between VFS consumers and external processes, and has been
 * replaced with the direct-to-host approach described above.
 *
 * Checkpoint/history recording is a separate concern that operates above
 * the VFS layer via the FileCheckpointManager in packages/common-utils.
 *
 * Tool integration:
 *   tools use WriteGuardVFS(writeIO: SandboxVFS) → VFSFileIO
 *   This splits reads (HostFSAdapter) and writes (SandboxVFS) at the
 *   VFSFileIO boundary without tool-level awareness.
 */

import * as path from "node:path";
import type { VFSEntry, VFSOperations } from "./types.js";
import type { VFSConfig, VFSProvider } from "@wf-agent/types";
import { MountTable } from "./internal/mount-table.js";
import { PathMapper } from "./internal/path-mapper.js";
import { WorkspaceSource } from "./internal/workspace-source.js";

export class SandboxVFS implements VFSOperations, VFSProvider {
  private workspaceRoot: string;
  private mountTable: MountTable;
  private base: WorkspaceSource;

  constructor(config: VFSConfig) {
    this.workspaceRoot = config.workspaceRoot;
    this.mountTable = new MountTable();
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
  // File operations — try MountTable first, then delegate to WorkspaceSource
  // which executes directly on the host filesystem with policy enforcement.
  // =========================================================================

  async stat(p: string): Promise<VFSEntry | null> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.stat(resolved.translatedPath);
    }
    return this.base.stat(this.toHostPath(p));
  }

  async readFile(p: string): Promise<Buffer | null> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.readFile(resolved.translatedPath);
    }
    return this.base.readFile(this.toHostPath(p));
  }

  async writeFile(p: string, data: Buffer): Promise<void> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.writeFile(resolved.translatedPath, data);
    }
    return this.base.writeFile(this.toHostPath(p), data);
  }

  async remove(p: string): Promise<void> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.remove(resolved.translatedPath);
    }
    return this.base.remove(this.toHostPath(p));
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    const resolvedOld = this.mountTable.resolve(oldPath);
    const resolvedNew = this.mountTable.resolve(newPath);

    // If both paths are in the same mount, delegate to that VFS
    if (resolvedOld && resolvedNew && resolvedOld.vfs === resolvedNew.vfs) {
      return resolvedOld.vfs.rename(resolvedOld.translatedPath, resolvedNew.translatedPath);
    }

    // One or both paths are in the base workspace
    return this.base.rename(this.toHostPath(oldPath), this.toHostPath(newPath));
  }

  // =========================================================================
  // Directory operations
  // =========================================================================

  async readdir(p: string): Promise<VFSEntry[]> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.readdir(resolved.translatedPath);
    }
    return this.base.readdir(this.toHostPath(p));
  }

  async mkdir(p: string): Promise<void> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.mkdir(resolved.translatedPath);
    }
    return this.base.mkdir(this.toHostPath(p));
  }

  async rmdir(p: string): Promise<void> {
    const resolved = this.mountTable.resolve(p);
    if (resolved) {
      return resolved.vfs.rmdir(resolved.translatedPath);
    }
    return this.base.rmdir(this.toHostPath(p));
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