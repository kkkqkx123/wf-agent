/**
 * File Monitoring Module
 *
 * Provides file monitoring and checkpoint management capabilities.
 * - FileCheckpointManager: orchestrates workspace scanning, hashing, and checkpoint creation/restoration
 * - FileCheckpointStorageAdapter: interface for persistence backends
 * - FileWatcher: chokidar-based incremental file change detection
 * - HashBaselineStore: hash-only baseline storage
 * - FileDeltaStore: incremental delta storage
 * - DiffEngine: Myers diff algorithm for text comparison
 */

export { FileCheckpointManager } from "./file-checkpoint-manager.js";
export type {
  FileCheckpointStorageAdapter,
  FileCheckpointCreateResult,
  FileCheckpointRestoreResult,
  FileCheckpointManagerConfig,
} from "./file-checkpoint-types.js";
export type { OptimizedFileCheckpointManagerConfig } from "./file-checkpoint-manager.js";

export { FileWatcher } from "./file-watcher.js";
export type {
  FileWatcherConfig,
  FileChangeRecord,
  FileChangeType,
  FileWatcherEvents,
} from "./file-watcher.js";

export { HashBaselineStore } from "./hash-baseline-store.js";
export type {
  HashBaseline,
  HashBaselineStoreConfig,
  HashBaselineStorageAdapter,
  FileHashRecord,
} from "./hash-baseline-store.js";

export { FileDeltaStore } from "./file-delta-store.js";
export type {
  FileDelta,
  FileDeltaChange,
  FileDeltaChangeType,
  FileDeltaStoreConfig,
  FileDeltaStorageAdapter,
} from "./file-delta-store.js";

export { DiffEngine } from "./diff-engine.js";
export type { DiffOp, DiffResult, DiffEngineConfig, DiffHunk } from "./diff-engine.js";
