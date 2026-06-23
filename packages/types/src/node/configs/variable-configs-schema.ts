/**
 * Zod Schemas for Variable Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Variable node configuration schema
 */
export const VariableNodeConfigSchema = z.object({
  variableName: z.string().min(1, "Variable name is required"),
  variableType: z.enum(["number", "string", "boolean", "array", "object"], {
    message: "Variable type must be one of: number, string, boolean, array, object",
  }),
  expression: z.string().min(1, "Expression is required"),
  scope: z.enum(["global", "workflowExecution", "subgraph", "loop"], {
    message: "Variable scope must be one of: global, workflowExecution, subgraph, loop",
  }).optional(),
  readonly: z.boolean().optional(),
});

/**
 * Type guard for runtime type checking
 */
export const isVariableNodeConfig = (config: unknown): config is z.infer<typeof VariableNodeConfigSchema> => {
  return VariableNodeConfigSchema.safeParse(config).success;
};