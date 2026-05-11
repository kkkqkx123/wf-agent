/**
 * Checkpoint Options Type Definition
 * Defines options for checkpoint save operations
 */

/**
 * Checkpoint creation options
 */
export interface CheckpointOptions {
  /**
   * If true, blocks until data is persisted to disk
   * Default: false (async for performance)
   */
  sync?: boolean;

  /**
   * Timeout for synchronous checkpoint (milliseconds)
   * Only applies when sync=true
   * Default: 30000 (30 seconds)
   */
  syncTimeout?: number;
}
