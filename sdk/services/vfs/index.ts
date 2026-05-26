/**
 * VFS Module — Unified Export (Phase 5)
 *
 * Virtual File System with Copy-on-Write, whiteout tracking, and snapshot support.
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Phase 5 delivers MemoryDelta; SQLiteDelta is planned for Phase 7.
 */

export type {
  VFSEntry,
  VFSOperations,
  DeltaFileSystem,
  BaseFileSystem,
  VFSConfig,
} from "./types.js";

export { WhiteoutCache } from "./whiteout-cache.js";
export { MemoryDelta } from "./delta/memory-delta.js";
export { HostFS } from "./base/host-fs.js";
export { OverlayVFS } from "./overlay-vfs.js";
export { CheckpointAwareVFS } from "./checkpoint-vfs.js";