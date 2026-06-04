/**
 * VFS Module — Public API
 *
 * Top-level VFS entry point for sandbox execution.
 * Internal implementations (WorkspaceSource, PathMapper, MountTable)
 * are not exported — they are consumed internally by SandboxVFS.
 *
 * Architecture: VFS as policy enforcement proxy, not a storage layer.
 * SandboxVFS executes operations directly on the host filesystem after
 * policy checking. See SandboxVFS doc comment for details.
 *
 * Exports:
 *   - SandboxVFS: Policy-enforcing VFS proxy backed by host filesystem
 *   - types:      Core interfaces (VFSOperations, VFSEntry)
 */

export type {
  VFSEntry,
  VFSOperations,
  VFSConfig,
} from "./types.js";

export type { VFSProvider } from "@wf-agent/types";

export { SandboxVFS } from "./sandbox-vfs.js";