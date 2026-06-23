/**
 * Zod Schemas for Sync Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import { WorkflowVariableInputSchema, WorkflowDataInputSchema, WorkflowMessageInputSchema } from "../../workflow/boundary-config-schema.js";

/**
 * Sync variable exchange schema
 */
const SyncVariableExchangeSchema = z.object({
  sourcePathId: z.string().min(1, "Exchange source path ID is required"),
  sourceVariable: z.string().min(1, "Exchange source variable name is required"),
  targetPathId: z.string().min(1, "Exchange target path ID is required"),
  targetVariable: z.string().min(1, "Exchange target variable name is required"),
});

/**
 * Sync node configuration schema
 */
export const SyncNodeConfigSchema = z.object({
  sourcePathId: z.string().min(1, "Source path ID is required"),
  targetPathId: z.string().min(1, "Target path ID is required").optional(),
  variableMappings: z.array(WorkflowVariableInputSchema).optional(),
  dataInputs: z.array(WorkflowDataInputSchema).optional(),
  messageInputs: z.array(WorkflowMessageInputSchema).optional(),
  waitForCompletion: z.boolean().optional().default(true),
  timeout: z.number().nonnegative("Timeout must be non-negative").optional().default(0),
  pairId: z.string().min(1, "Pair ID is required for paired SYNC").optional(),
  variableExchanges: z.array(SyncVariableExchangeSchema).optional(),
});

/**
 * Type guard for runtime type checking
 */
export const isSyncNodeConfig = (config: unknown): config is z.infer<typeof SyncNodeConfigSchema> => {
  return SyncNodeConfigSchema.safeParse(config).success;
};
