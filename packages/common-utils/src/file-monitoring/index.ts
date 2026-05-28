/**
 * File Monitoring Module
 *
 * Provides file monitoring and checkpoint management capabilities.
 * - FileCheckpointManager: orchestrates workspace scanning, hashing, and checkpoint creation/restoration
 * - FileCheckpointStorageAdapter: interface for persistence backends
 */

export { FileCheckpointManager } from './file-checkpoint-manager.js';
export type {
  FileCheckpointStorageAdapter,
  FileCheckpointCreateResult,
  FileCheckpointRestoreResult,
  FileCheckpointManagerConfig,
} from './file-checkpoint-types.js';