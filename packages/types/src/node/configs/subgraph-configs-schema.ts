/**
 * Zod Schemas for Subgraph Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 * 
 * Note: Reuses WorkflowVariableInputSchema and WorkflowVariableOutputSchema from
 * workflow/boundary-config-schema.ts for consistency across all boundary configurations.
 */

import { z } from "zod";
import { WorkflowVariableInputSchema, WorkflowVariableOutputSchema, WorkflowDataInputSchema, WorkflowMessageInputSchema, WorkflowMessageOutputSchema } from "../../workflow/boundary-config-schema.js";

/**
 * Subgraph node configuration schema
 */
export const SubgraphNodeConfigSchema = z.object({
  subgraphId: z.string().min(1, "Subgraph ID is required"),
  async: z.boolean(),
  onFailure: z
    .enum(["fail", "continue", "retry"], {
      message: "onFailure must be one of: fail, continue, retry",
    })
    .optional(),
  maxRetries: z
    .number()
    .int()
    .positive("maxRetries must be positive")
    .optional(),
  retryDelayMs: z
    .number()
    .int()
    .nonnegative("retryDelayMs must be non-negative")
    .optional(),
  fallbackOutput: z.record(z.string(), z.unknown()).optional(),
  variableInputs: z.array(WorkflowVariableInputSchema).optional(),
  variableOutputs: z.array(WorkflowVariableOutputSchema).optional(),
  dataInputs: z.array(WorkflowDataInputSchema).optional(),
  messagePassing: z.object({
    inputs: z.array(WorkflowMessageInputSchema).optional(),
    outputs: z.array(WorkflowMessageOutputSchema).optional(),
  }).optional(),
}).refine(
  (data) => {
    // retry strategy requires maxRetries
    if (data.onFailure === "retry" && (data.maxRetries === undefined || data.maxRetries === null)) {
      return false;
    }
    return true;
  },
  {
    message: "maxRetries is required when onFailure is retry",
    path: ["maxRetries"],
  },
);

/**
 * Type guard for runtime type checking
 */
export const isSubgraphNodeConfig = (config: unknown): config is z.infer<typeof SubgraphNodeConfigSchema> => {
  return SubgraphNodeConfigSchema.safeParse(config).success;
};
