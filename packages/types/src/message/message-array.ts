/**
 * Message Array Type Definition
 * Defines the status and statistics of the message array
 */

import type { Message } from "./message.js";
import type { BatchSnapshot } from "./batch-snapshot.js";

/**
 * Message Array Status
 */
export interface MessageArrayState {
  /** Complete message array (contains all batches) */
  messages: Message[];
  /** Batch snapshot array */
  batchSnapshots: BatchSnapshot[];
  /** Current Batch Index */
  currentBatchIndex: number;
  /** Total number of messages */
  totalMessageCount: number;
}

/**
 * Message array statistics
 */
export interface MessageArrayStats {
  /** Total messages */
  totalMessages: number;
  /** Number of messages in current batch */
  currentBatchMessages: number;
  /** Total number of batches */
  totalBatches: number;
  /** Current Batch Index */
  currentBatchIndex: number;
}
