/**
 * Zod Schemas for Subgraph Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Subgraph node configuration schema
 */
export const SubgraphNodeConfigSchema = z.object({
  subgraphId: z.string().min(1, "Subgraph ID is required"),
  async: z.boolean(),
  messagePassing: z.object({
    inputs: z.record(z.string(), z.string()).optional(),
    outputs: z.record(z.string(), z.string()).optional(),
  }).optional(),
});

/**
 * Type guard for runtime type checking
 */
export const isSubgraphNodeConfig = (config: unknown): config is z.infer<typeof SubgraphNodeConfigSchema> => {
  return SubgraphNodeConfigSchema.safeParse(config).success;
};
