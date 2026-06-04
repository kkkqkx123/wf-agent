/**
 * VFS Module — Public API
 *
 * Top-level VFS entry point for sandbox execution.
 * Internal implementations (DeltaStore, WorkspaceSource, PathMapper, MountTable)
 * are not exported — they are consumed internally by SandboxVFS.
 *
 * Layer classification:
 *   - SandboxVFS: Top-level sandbox VFS with delta-over-base layering
 *   - types:      Core interfaces (VFSOperations, DeltaFileSystem, Snapshotable, VFSEntry)
 */

export type {
  VFSEntry,
  VFSOperations,
  DeltaFileSystem,
  VFSConfig,
} from "./types.js";

export type { VFSProvider } from "@wf-agent/types";

export { SandboxVFS } from "./sandbox-vfs.js";