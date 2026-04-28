/**
 * Unified Export for Message Management Module
 * Export all message-related type definitions
 */

// Message base type
export type { Message, MessageContent, LLMMessage, LLMToolCall } from "./message.js";

export { MessageRole } from "./message.js";

// Batch snapshot type
export type { BatchSnapshot, BatchSnapshotArray } from "./batch-snapshot.js";

// Message Array Type
export type { MessageArrayState, MessageArrayStats } from "./message-array.js";

// Message Operation Type
export type {
  MessageOperationType,
  MessageOperationConfig,
  AppendMessageOperation,
  InsertMessageOperation,
  ReplaceMessageOperation,
  TruncateMessageOperation,
  ClearMessageOperation,
  FilterMessageOperation,
  RollbackMessageOperation,
  MessageOperationResult,
} from "./message-operations.js";

// Message Operation Context Type
export type { MessageOperationContext } from "./message-context.js";

// Message token mapping type
export type { MessageMarkMap, BatchCheckpointInfo, MemoryRangeConfig } from "./message-mark-map.js";

// Batch management operation type
export type {
  BatchManagementOperation,
  BatchManagementOperationType,
} from "./batch-management-operation.js";

// Zod Schemas for Message Operations
export {
  MessageOperationConfigSchema,
  AppendMessageOperationSchema,
  InsertMessageOperationSchema,
  ReplaceMessageOperationSchema,
  TruncateMessageOperationSchema,
  ClearMessageOperationSchema,
  FilterMessageOperationSchema,
  RollbackMessageOperationSchema,
  isAppendMessageOperation,
  isInsertMessageOperation,
  isReplaceMessageOperation,
  isTruncateMessageOperation,
  isClearMessageOperation,
  isFilterMessageOperation,
  isRollbackMessageOperation,
} from "./message-operations-schema.js";
