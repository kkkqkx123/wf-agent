/**
 * VFS — Types
 *
 * Architecture: VFS as policy enforcement proxy, not a storage layer.
 *
 * SandboxVFS focuses on write-path policy enforcement:
 *   - Read operations are pass-through to the host filesystem (no policy check,
 *     no overhead). In the tool chain, reads go through HostFSAdapter directly.
 *   - Write operations are checked against path policy and rejected immediately
 *     if denied, before any filesystem mutation.
 *
 * This ensures:
 *   - Tools and external processes (compilation, shell commands) always
 *     see the same file state — there is no "hidden" VFS layer.
 *   - Policy violations are caught before any filesystem mutation occurs.
 *   - No sync mechanism is needed because the host FS is the source of truth.
 *   - Read operations incur zero overhead (no intermediate layer).
 *
 * Tool integration:
 *   - Tools use WriteGuardVFS(writeIO: SandboxVFS) → VFSFileIO
 *   - Reads: HostFSAdapter → direct fs (no VFS overhead)
 *   - Writes: SandboxVFS → policy check → host fs (safety guarantee)
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
 * SandboxVFS operates as a policy proxy: all operations are checked against
 * the path policy and, if allowed, executed directly on the host filesystem.
 * There is no intermediate storage or delta layering in this path.
 *
 * PathMapper (mount table entries) provides path translation between sandbox
 * virtual paths and host paths, with direct pass-through to the host FS.
 *
 * WorkspaceSource provides host FS access with path policy filtering.
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
