/**
 * Zod Schemas for Subgraph Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 * 
 * Note: Reuses WorkflowVariableInputSchema and WorkflowVariableOutputSchema from
 * workflow/boundary-config-schema.ts for consistency across all boundary configurations.
 */

import { z } from "zod";
import { WorkflowVariableInputSchema, WorkflowVariableOutputSchema, WorkflowDataInputSchema } from "../../workflow/boundary-config-schema.js";

/**
 * Subgraph node configuration schema
 */
export const SubgraphNodeConfigSchema = z.object({
  subgraphId: z.string().min(1, "Subgraph ID is required"),
  async: z.boolean(),
  variableInputs: z.array(WorkflowVariableInputSchema).optional(),
  variableOutputs: z.array(WorkflowVariableOutputSchema).optional(),
  dataInputs: z.array(WorkflowDataInputSchema).optional(),
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
