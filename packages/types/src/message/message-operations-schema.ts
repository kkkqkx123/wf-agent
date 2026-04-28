/**
 * Zod Schemas for Message Operation Types
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Base message schema (simplified for validation)
 */
const messageSchema = z.object({
  role: z.enum(["system", "user", "assistant", "tool"]),
  content: z.any(),
  toolCalls: z.array(z.any()).optional(),
  toolCallId: z.string().optional(),
});

/**
 * Truncation strategy schema
 */
const truncateStrategySchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("KEEP_FIRST"),
    count: z.number().int().nonnegative(),
  }),
  z.object({
    type: z.literal("KEEP_LAST"),
    count: z.number().int().nonnegative(),
  }),
  z.object({
    type: z.literal("REMOVE_FIRST"),
    count: z.number().int().nonnegative(),
  }),
  z.object({
    type: z.literal("REMOVE_LAST"),
    count: z.number().int().nonnegative(),
  }),
  z.object({
    type: z.literal("RANGE"),
    start: z.number().int().nonnegative(),
    end: z.number().int().positive(),
  }),
]);

/**
 * APPEND operation schema
 */
export const AppendMessageOperationSchema = z.object({
  operation: z.literal("APPEND"),
  messages: z.array(messageSchema).min(1, "Messages array cannot be empty"),
});

/**
 * INSERT operation schema
 */
export const InsertMessageOperationSchema = z.object({
  operation: z.literal("INSERT"),
  position: z.number().int().nonnegative(),
  messages: z.array(messageSchema).min(1, "Messages array cannot be empty"),
  createNewBatch: z.boolean().optional(),
});

/**
 * REPLACE operation schema
 */
export const ReplaceMessageOperationSchema = z.object({
  operation: z.literal("REPLACE"),
  index: z.number().int().nonnegative(),
  message: messageSchema,
  createNewBatch: z.boolean().optional(),
});

/**
 * TRUNCATE operation schema
 */
export const TruncateMessageOperationSchema = z.object({
  operation: z.literal("TRUNCATE"),
  strategy: truncateStrategySchema,
  role: z.enum(["system", "user", "assistant", "tool"]).optional(),
  createNewBatch: z.boolean().optional(),
});

/**
 * CLEAR operation schema
 */
export const ClearMessageOperationSchema = z.object({
  operation: z.literal("CLEAR"),
  createNewBatch: z.boolean().optional(),
});

/**
 * FILTER operation schema
 */
export const FilterMessageOperationSchema = z.object({
  operation: z.literal("FILTER"),
  roles: z.array(z.enum(["system", "user", "assistant", "tool"])).optional(),
  contentContains: z.array(z.string()).min(1).optional(),
  contentExcludes: z.array(z.string()).min(1).optional(),
  createNewBatch: z.boolean().optional(),
});

/**
 * ROLLBACK operation schema
 */
export const RollbackMessageOperationSchema = z.object({
  operation: z.literal("ROLLBACK"),
  targetBatchIndex: z.number().int().nonnegative(),
});

/**
 * Message operation config union schema
 */
export const MessageOperationConfigSchema = z.discriminatedUnion("operation", [
  AppendMessageOperationSchema,
  InsertMessageOperationSchema,
  ReplaceMessageOperationSchema,
  TruncateMessageOperationSchema,
  ClearMessageOperationSchema,
  FilterMessageOperationSchema,
  RollbackMessageOperationSchema,
]);

/**
 * Type guards for runtime type checking
 */
export const isAppendMessageOperation = (
  config: unknown,
): config is z.infer<typeof AppendMessageOperationSchema> => {
  return AppendMessageOperationSchema.safeParse(config).success;
};

export const isInsertMessageOperation = (
  config: unknown,
): config is z.infer<typeof InsertMessageOperationSchema> => {
  return InsertMessageOperationSchema.safeParse(config).success;
};

export const isReplaceMessageOperation = (
  config: unknown,
): config is z.infer<typeof ReplaceMessageOperationSchema> => {
  return ReplaceMessageOperationSchema.safeParse(config).success;
};

export const isTruncateMessageOperation = (
  config: unknown,
): config is z.infer<typeof TruncateMessageOperationSchema> => {
  return TruncateMessageOperationSchema.safeParse(config).success;
};

export const isClearMessageOperation = (
  config: unknown,
): config is z.infer<typeof ClearMessageOperationSchema> => {
  return ClearMessageOperationSchema.safeParse(config).success;
};

export const isFilterMessageOperation = (
  config: unknown,
): config is z.infer<typeof FilterMessageOperationSchema> => {
  return FilterMessageOperationSchema.safeParse(config).success;
};

export const isRollbackMessageOperation = (
  config: unknown,
): config is z.infer<typeof RollbackMessageOperationSchema> => {
  return RollbackMessageOperationSchema.safeParse(config).success;
};
