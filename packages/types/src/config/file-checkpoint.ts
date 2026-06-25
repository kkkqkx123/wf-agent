/**
 * File Checkpoint Configuration Type Definitions
 *
 * Defines user-facing configuration types for the file checkpoint subsystem.
 * These types are the declarative configuration that users provide through SDKOptions or config files.
 * The actual runtime FileCheckpointManagerConfig (in packages/storage) is derived from these.
 */

/**
 * File checkpoint storage backend configuration
 */
export interface FileCheckpointStorageConfig {
  /** Storage backend type */
  type: "sqlite" | "json";
  /** Database file path (only for sqlite type) */
  dbPath?: string;
}

/**
 * File checkpoint configuration
 *
 * Controls the workspace file state checkpointing feature.
 * When enabled, file checkpoints are created alongside execution checkpoints
 * to capture workspace file state for restoration.
 */
export interface FileCheckpointConfig {
  /** Whether file checkpointing is enabled (default: false) */
  enabled: boolean;
  /**
   * Workspace root path to snapshot.
   * If not set, defaults to process.cwd() at initialization time.
   */
  workspaceRoot?: string;
  /** Maximum delta chain length before forcing a full backup (default: 20) */
  maxDeltaChainLength?: number;
  /** Custom ignore patterns (appended to .gitignore and hardcoded ignores) */
  customIgnorePatterns?: string[];
  /** Storage backend configuration */
  storage?: FileCheckpointStorageConfig;
  /**
   * Error handling behavior for file checkpoint operations.
   * - "warn": Log warning and continue (default, backward compatible)
   * - "error": Throw error, allowing caller to handle
   * - "ignore": Silently ignore errors
   */
  failureBehavior?: "warn" | "error" | "ignore";
}
