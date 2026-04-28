/**
 * Zod Schemas for Context Processor Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import { MessageOperationConfigSchema } from "../../message/message-operations-schema.js";

/**
 * Context processor node configuration schema
 */
export const ContextProcessorNodeConfigSchema = z.object({
  version: z.number().optional(),
  operationConfig: MessageOperationConfigSchema,
  operationOptions: z.object({
    visibleOnly: z.boolean().optional(),
    autoCreateBatch: z.boolean().optional(),
    target: z.enum(["self", "parent"]).optional(),
  }).optional(),
});

/**
 * Type guard for runtime type checking
 */
export const isContextProcessorNodeConfig = (config: unknown): config is z.infer<typeof ContextProcessorNodeConfigSchema> => {
  return ContextProcessorNodeConfigSchema.safeParse(config).success;
};
