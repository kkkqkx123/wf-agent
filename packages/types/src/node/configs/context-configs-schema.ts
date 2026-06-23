/**
 * Zod Schemas for Context Processor / Data Processor Node Configuration
 * Provides runtime validation schemas synchronized with TypeScript type definitions
 */

import { z } from "zod";
import { MessageOperationConfigSchema } from "../../message/message-operations-schema.js";
import { VariableOperationConfigSchema } from "./variable-operation-schema.js";

/**
 * Message operation output schema
 */
export const MessageOperationOutputSchema = z.object({
  operation: z.string(),
  messageCount: z.number().min(0),
  sourceContext: z.string(),
  targetContext: z.string(),
  stats: z.object({
    originalMessageCount: z.number().min(0),
    visibleMessageCount: z.number().min(0),
    invisibleMessageCount: z.number().min(0),
  }).optional(),
});

/**
 * Variable operation output schema
 */
export const VariableOperationOutputSchema = z.object({
  operation: z.string(),
  modifiedVariables: z.array(
    z.object({
      name: z.string(),
      newValue: z.unknown(),
      type: z.string().optional(),
    }),
  ),
  executionTime: z.number().min(0),
  stats: z.object({
    sourceVariableCount: z.number().optional(),
    aggregatedItemCount: z.number().optional(),
  }).optional(),
});

/**
 * Context processor node configuration schema
 */
export const ContextProcessorNodeConfigSchema = z.object({
  version: z.number().optional(),
  operationConfig: MessageOperationConfigSchema.optional(),
  variableOperation: VariableOperationConfigSchema.optional(),
  sourceContext: z.string().optional(),
  targetContext: z.string().optional(),
  operationOptions: z.object({
    visibleOnly: z.boolean().optional(),
    autoCreateBatch: z.boolean().optional(),
    target: z.enum(["self", "parent"]).optional(),
  }).optional(),
}).refine(
  (config) => config.operationConfig || config.variableOperation,
  "At least one operation (message or variable) must be specified"
);

/**
 * Type guard for runtime type checking
 */
export const isContextProcessorNodeConfig = (config: unknown): config is z.infer<typeof ContextProcessorNodeConfigSchema> => {
  return ContextProcessorNodeConfigSchema.safeParse(config).success;
};
