/**
 * Batch Management Operation Type Definition
 * Define batch management related operation configurations
 */

import type { MessageOperationConfig } from "./message-operations.js";

/**
 * Batch management operation type
 */
export type BatchManagementOperationType =
  | "START_NEW_BATCH" // Start a new batch
  | "ROLLBACK_TO_BATCH"; // Fall back to specified batch

/**
 * Batch Management Operation Configuration
 */
export interface BatchManagementOperation extends MessageOperationConfig {
  operation: "BATCH_MANAGEMENT";
  /** Batch management operation type */
  batchOperation: BatchManagementOperationType;
  /** Target Batch Index (for ROLLBACK_TO_BATCH) */
  targetBatch?: number;
  /** Boundary index (for START_NEW_BATCH) */
  boundaryIndex?: number;
}
