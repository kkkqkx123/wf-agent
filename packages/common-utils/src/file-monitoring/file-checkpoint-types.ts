/**
 * File Checkpoint Types
 *
 * Business types and storage adapter interface for workspace file checkpointing.
 * Lives in common-utils because:
 * - FileCheckpointManager (business logic) needs these types
 * - Storage implementations consume the adapter interface
 * - common-utils is the lowest shared layer
 */

import type { FileCheckpointMetadata, FileCheckpointListOptions } from "@wf-agent/types";

/**
 * File checkpoint storage adapter interface
 * Handles persistence of file checkpoint metadata and file contents.
 */
export interface FileCheckpointStorageAdapter {
  initialize(): Promise<void>;
  close(): Promise<void>;
  clear(): Promise<void>;

  save(id: string, metadata: FileCheckpointMetadata, files: Map<string, Buffer>): Promise<void>;

  load(id: string): Promise<{ metadata: FileCheckpointMetadata; files: Map<string, Buffer> } | null>;

  delete(id: string): Promise<void>;

  list(options?: FileCheckpointListOptions): Promise<string[]>;

  listByEntity(
    entityId: string,
    options?: { limit?: number },
  ): Promise<Array<{ id: string; metadata: FileCheckpointMetadata }>>;

  getLatestByEntity(
    entityId: string,
  ): Promise<{ id: string; metadata: FileCheckpointMetadata; files?: Map<string, Buffer> } | null>;

  deleteByEntity(entityId: string, keepLatest?: number): Promise<number>;
}

/**
 * Optimized FileCheckpointManager configuration
 */
export interface OptimizedFileCheckpointManagerConfig extends FileCheckpointManagerConfig {
  /** Enable file watching for incremental change detection */
  useFileWatcher?: boolean;
  /** Enable hash baseline storage (reduces initial storage) */
  useHashBaseline?: boolean;
  /** Enable diff generation for changed files */
  enableDiff?: boolean;
}

/**
 * File checkpoint error handling configuration
 */
export interface FileCheckpointErrorConfig {
  /**
   * Error handling behavior for file checkpoint operations.
   * - "warn": Log warning and continue (default, backward compatible)
   * - "error": Throw error, allowing caller to handle
   * - "ignore": Silently ignore errors
   */
  failureBehavior: "warn" | "error" | "ignore";
}

/**
 * File checkpoint create result
 */
export interface FileCheckpointCreateResult {
  id: string;
  metadata: FileCheckpointMetadata;
}

/**
 * File checkpoint restore result
 */
export interface FileCheckpointRestoreResult {
  restoredCount: number;
  deletedCount: number;
  skippedCount: number;
}

/**
 * File checkpoint manager configuration
 */
export interface FileCheckpointManagerConfig {
  enabled: boolean;
  workspaceRoot: string;
  customIgnorePatterns?: string[];
  maxFileSize?: number;
  maxDeltaChainLength?: number;
  /**
   * Error handling behavior for file checkpoint operations.
   * - "warn": Log warning and continue (default, backward compatible)
   * - "error": Throw error, allowing caller to handle
   * - "ignore": Silently ignore errors
   */
  failureBehavior?: "warn" | "error" | "ignore";
}