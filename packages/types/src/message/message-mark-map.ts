/**
 * Message Tag Mapping Type Definition
 * Defines the data structure of the message token mapping used to track message visibility and batch information
 */

/**
 * Batch Checkpoint Info
 * Maps a batch to its checkpoint for memory optimization
 */
export interface BatchCheckpointInfo {
  /** Batch ID */
  batchId: number;
  /** Checkpoint ID (null if messages are still in memory) */
  checkpointId: string | null;
  /** Number of messages in this batch */
  messageCount: number;
  /** Timestamp when checkpoint was created */
  timestamp?: number;
}

/**
 * Memory Range Configuration
 * Defines which batches should be kept in memory
 */
export interface MemoryRangeConfig {
  /** Start batch index in memory */
  startBatch: number;
  /** End batch index in memory */
  endBatch: number;
}

/**
 * Message Tag Mapping
 * For tracking message visibility and batch boundaries
 *
 * Design Notes:
 * - Only raw indexes and batch information are stored, not type indexes
 * - Type index is calculated to avoid data redundancy.
 * - Single data source to ensure consistency
 * - Supports batch-to-checkpoint mapping for memory optimization
 */
export interface MessageMarkMap {
  /** Array of original indexes (to record the original location of all messages) */
  originalIndices: number[];
  /** Array of batch boundaries (starting index of each batch) */
  batchBoundaries: number[];
  /** Boundary-to-batch mapping */
  boundaryToBatch: number[];
  /** Current Batch Index */
  currentBatch: number;
  /**
   * Batch to checkpoint mapping for memory optimization
   * When a batch is offloaded to checkpoint, its checkpointId is set
   * Null checkpointId means messages are still in memory
   */
  batchToCheckpoint?: BatchCheckpointInfo[];
  /**
   * Memory range configuration
   * Defines which batches are currently loaded in memory
   */
  memoryRange?: MemoryRangeConfig;
}
