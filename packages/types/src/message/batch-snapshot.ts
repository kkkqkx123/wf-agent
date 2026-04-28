/**
 * Batch Snapshot Type Definition
 * Defines the data structure of a batch snapshot for storing historical state
 */

import type { Message } from "./message.js";

/**
 * Batch Snapshot
 * Stores the full message array state at the time of batch creation
 */
export interface BatchSnapshot {
  /** Batch Index */
  batchIndex: number;
  /** Batch creation timestamp */
  timestamp: number;
  /** Deep copy of batch message array (if empty array means no additional copying overhead) */
  messages: Message[];
  /** Number of batch messages */
  messageCount: number;
  /** Batch description */
  description?: string;
}

/**
 * Batch snapshot array
 */
export type BatchSnapshotArray = BatchSnapshot[];
