/**
 * Zod Schemas for Subgraph Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Variable input mapping schema
 */
const variableInputSchema = z.object({
  externalName: z.string().min(1, "External name (parent variable) is required"),
  internalName: z.string().min(1, "Internal name (subgraph variable) is required"),
  required: z.boolean().optional(),
  defaultValue: z.any().optional(),
  description: z.string().optional(),
});

/**
 * Variable output mapping schema
 */
const variableOutputSchema = z.object({
  internalName: z.string().min(1, "Internal name (subgraph variable) is required"),
  externalName: z.string().min(1, "External name (parent variable) is required"),
  description: z.string().optional(),
});

/**
 * Subgraph node configuration schema
 */
export const SubgraphNodeConfigSchema = z.object({
  subgraphId: z.string().min(1, "Subgraph ID is required"),
  async: z.boolean(),
  variableInputs: z.array(variableInputSchema).optional(),
  variableOutputs: z.array(variableOutputSchema).optional(),
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
