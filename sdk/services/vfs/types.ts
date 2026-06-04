/**
 * VFS — Types
 *
 * Core interfaces for the Virtual File System layer.
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Layer classification:
 *   - VFSOperations: Unified file system interface for all VFS implementations
 *   - DeltaFileSystem: Writable overlay contract used by SandboxVFS internally
 *   - Snapshotable: Optional capability for delta layers (checkpoint integration)
 */

import type { VFSConfig } from "@wf-agent/types";

export type { VFSConfig };

/**
 * Virtual file system entry metadata.
 */
export interface VFSEntry {
  name: string;
  path: string;
  type: "file" | "directory";
  size: number;
  mode: number;
  createdAt: number;
  modifiedAt: number;
}

/**
 * Core VFS operations.
 * All paths are absolute within the VFS namespace.
 *
 * This is the unified interface that all VFS implementations
 * (SandboxVFS, PathMapper, WorkspaceSource) must satisfy.
 * It abstracts away the underlying storage backend — whether
 * Host FS, SQLite, or in-memory.
 */
export interface VFSOperations {
  stat(path: string): Promise<VFSEntry | null>;
  readFile(path: string): Promise<Buffer | null>;
  writeFile(path: string, data: Buffer): Promise<void>;
  remove(path: string): Promise<void>;
  rename(oldPath: string, newPath: string): Promise<void>;

  readdir(path: string): Promise<VFSEntry[]>;
  mkdir(path: string): Promise<void>;
  rmdir(path: string): Promise<void>;

  exists(path: string): Promise<boolean>;
  isAllowed(path: string): boolean;

  /**
   * Translate a sandbox virtual path to the actual backend path.
   * For PathMapper, this returns the host filesystem path.
   * For SandboxVFS, this returns the VFS-normalized path.
   * Returns the original path if no translation is needed.
   */
  translatePath(path: string): string;
}

/**
 * Delta layer interface — writable overlay contract used internally by SandboxVFS.
 *
 * Each delta implementation (memory, SQLite) must implement this.
 * Unlike VFSOperations, the delta layer does not include path policy checks
 * or path translation — those are handled by the VFS layer above.
 */
export interface DeltaFileSystem {
  stat(path: string): Promise<VFSEntry | null>;
  readFile(path: string): Promise<Buffer | null>;
  writeFile(path: string, data: Buffer): Promise<void>;
  remove(path: string): Promise<void>;
  rename(oldPath: string, newPath: string): Promise<void>;

  readdir(path: string): Promise<VFSEntry[]>;
  mkdir(path: string): Promise<void>;
  rmdir(path: string): Promise<void>;

  /** Create a symbolic link. target is the existing file, path is the symlink name. */
  symlink(target: string, path: string): Promise<void>;
  /** Read a symbolic link target. */
  readlink(path: string): Promise<string | null>;
}

/**
 * Snapshotable interface for delta layers that support checkpoint integration.
 *
 * Enables VFS snapshot/restore bridging with the checkpoint system.
 * This is an optional capability — not all delta layers implement it.
 *
 * Note: This interface is consumed internally by SandboxVFS and not
 * intended for direct use by external consumers. Checkpoint integration
 * should be done via SandboxVFS.snapshot() / SandboxVFS.restore().
 */
export interface Snapshotable {
  snapshot(): Promise<string>;
  restore(snapshotId: string): Promise<void>;
  listSnapshots(): Promise<Array<{ id: string; createdAt: number }>>;
  deleteSnapshot(snapshotId: string): Promise<void>;
}
