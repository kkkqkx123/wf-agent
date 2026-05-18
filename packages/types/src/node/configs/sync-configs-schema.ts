/**
 * Zod Schemas for Sync Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import { WorkflowVariableInputSchema } from "../../workflow/boundary-config-schema.js";

/**
 * Sync node configuration schema
 */
export const SyncNodeConfigSchema = z.object({
  sourcePathId: z.string().min(1, "Source path ID is required"),
  targetPathId: z.string().min(1, "Target path ID is required").optional(),
  variableMappings: z.array(WorkflowVariableInputSchema).optional(),
  waitForCompletion: z.boolean().optional().default(true),
  timeout: z.number().nonnegative("Timeout must be non-negative").optional().default(0),
});

/**
 * Type guard for runtime type checking
 */
export const isSyncNodeConfig = (config: unknown): config is z.infer<typeof SyncNodeConfigSchema> => {
  return SyncNodeConfigSchema.safeParse(config).success;
};
