/**
 * Message Operation Type Definitions
 * Defines the configuration and result types for all message operations
 */

import type { Message } from "./message.js";
import type { MessageMarkMap } from "./message-mark-map.js";
import { MessageRole } from "./message.js";

/**
 * Message Operation Type
 */
export type MessageOperationType =
  | "APPEND" // Tailgate messages (no new batches are created)
  | "INSERT" // Intermediate insertion message (create new batch)
  | "REPLACE" // Replacement message (create new batch)
  | "TRUNCATE" // Truncate messages (create new batches)
  | "CLEAR" // Empty message (create new batch with empty snapshot)
  | "FILTER" // Filtering messages (creating new batches)
  | "ROLLBACK" // Fallback to a specified batch
  | "BATCH_MANAGEMENT"; // Batch management operations

/**
 * Message Operations Configuration Base Interface
 */
export interface MessageOperationConfig {
  /** Type of operation */
  operation: MessageOperationType;
}

/**
 * APPEND Operation Configuration
 */
export interface AppendMessageOperation extends MessageOperationConfig {
  operation: "APPEND";
  /** Array of messages to append */
  messages: Message[];
}

/**
 * INSERT operation configuration
 */
export interface InsertMessageOperation extends MessageOperationConfig {
  operation: "INSERT";
  /** Insert position (relative to current batch, 0 <= position <= currentBatchMessages.length) */
  position: number;
  /** Array of messages to insert */
  messages: Message[];
  /** Whether to start a new batch after insertion */
  createNewBatch?: boolean;
}

/**
 * REPLACE operation configuration
 */
export interface ReplaceMessageOperation extends MessageOperationConfig {
  operation: "REPLACE";
  /** Index of the message to replace (relative to the current batch) */
  index: number;
  /** New message content */
  message: Message;
  /** Whether to start a new batch after replacement */
  createNewBatch?: boolean;
}

/**
 * TRUNCATE operation configuration
 */
export interface TruncateMessageOperation extends MessageOperationConfig {
  operation: "TRUNCATE";

  /** Truncation strategy (enumeration) */
  strategy:
    | { type: "KEEP_FIRST"; count: number }
    | { type: "KEEP_LAST"; count: number }
    | { type: "REMOVE_FIRST"; count: number }
    | { type: "REMOVE_LAST"; count: number }
    | { type: "RANGE"; start: number; end: number };

  /** Role filtering (filter out messages for a given role before performing a truncation) */
  role?: MessageRole;

  /** Whether to start a new batch after truncation */
  createNewBatch?: boolean;
}

/**
 * CLEAR Operation Configuration
 * Note: The SDK provides an atomic operation for complete emptying.
 * If you need to keep specific messages (e.g., system messages), you should filter them with FILTER at the application level before CLEARing them.
 */
export interface ClearMessageOperation extends MessageOperationConfig {
  operation: "CLEAR";
  /** Whether to start a new batch after emptying */
  createNewBatch?: boolean;
}

/**
 * FILTER Operation Configuration
 */
export interface FilterMessageOperation extends MessageOperationConfig {
  operation: "FILTER";
  /** Filter by Role */
  roles?: Message["role"][];
  /** Filtering by content keywords (messages containing specified keywords) */
  contentContains?: string[];
  /** Exclusion by content keywords (messages that do not contain the specified keywords) */
  contentExcludes?: string[];
  /** Whether to start a new batch after filtering */
  createNewBatch?: boolean;
}

/**
 * ROLLBACK operation configuration
 */
export interface RollbackMessageOperation extends MessageOperationConfig {
  operation: "ROLLBACK";
  /** Target Batch Index */
  targetBatchIndex: number;
}

/**
 * Message Operation Result
 */
export interface MessageOperationResult {
  /** Array of messages after the operation */
  messages: Message[];
  /** Post-operation marker mapping */
  markMap: MessageMarkMap;
  /** Batch indexes affected by the operation */
  affectedBatchIndex: number;
  /** Operational Statistics */
  stats: {
    originalMessageCount: number;
    visibleMessageCount: number;
    invisibleMessageCount: number;
  };
}
