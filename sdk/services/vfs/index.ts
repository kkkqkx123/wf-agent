/**
 * VFS Module — Unified Export
 *
 * Virtual File System with SQLite-backed delta layer.
 */

export type {
  VFSEntry,
  VFSOperations,
  DeltaFileSystem,
  BaseFileSystem,
  VFSConfig,
} from "./types.js";

export { HostFS } from "./base/host-fs.js";
export { OverlayVFS } from "./overlay-vfs.js";
export { SqliteDelta } from "./delta/sqlite-delta.js";