/**
 * Zod Schemas for Loop Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Loop variable input mapping schema
 */
const loopVariableInputSchema = z.object({
  externalName: z.string().min(1, "External name (parent variable) is required"),
  internalName: z.string().min(1, "Internal name (loop variable) is required"),
  required: z.boolean().optional(),
  defaultValue: z.any().optional(),
  description: z.string().optional(),
});

/**
 * Loop data source schema
 *
 * DataSource supports two forms:
 * 1. Direct value: array, object, number, string
 * 2. Variable expression string: {{variable.path}} form
 *
 * Note: The actual parsing and validation of variable expressions are handled by loopStartHandler at runtime.
 */
const dataSourceSchema = z.object({
  iterable: z.any().refine((val) => val !== undefined && val !== null, "Iterable is required"),
  variableName: z.string().min(1, "Variable name is required"),
});

/**
 * Loop start node configuration schema
 *
 * Supports two modes:
 * 1. Data-driven loop: Provides dataSource (which includes iterable and variableName)
 * 2. Count-driven loop: Does not provide dataSource, uses only maxIterations
 *
 * IMPORTANT: Loops do NOT inherit parent workflow variables automatically.
 * All variables needed inside the loop must be explicitly declared in variableInputs.
 */
export const LoopStartNodeConfigSchema = z.object({
  loopId: z.string().min(1, "Loop ID is required"),
  variableInputs: z.array(loopVariableInputSchema).optional(),
  dataSource: dataSourceSchema.optional(),
  maxIterations: z.number().positive("Max iterations must be positive"),
});

/**
 * Loop end node configuration schema
 *
 * Description: Check the loop condition and break condition to decide whether to continue iteration or not.
 * - loopId uniquely identifies the loop and is used to retrieve the loop state initialized in LOOP_START.
 * - Loop state (iterable, iterationCount, etc.) is already initialized and stored in LOOP_START, no need to define it again.
 * - All loop data and state are in loop-level scopes, isolated from other scopes.
 */
export const LoopEndNodeConfigSchema = z.object({
  loopId: z.string().min(1, "Loop ID is required"),
  breakCondition: z
    .object({
      expression: z.string().min(1, "Break condition expression is required"),
      metadata: z.any().optional(),
    })
    .optional(),
  loopStartNodeId: z.string().optional(),
});

/**
 * Type guards for runtime type checking
 */
export const isLoopStartNodeConfig = (config: unknown): config is z.infer<typeof LoopStartNodeConfigSchema> => {
  return LoopStartNodeConfigSchema.safeParse(config).success;
};

export const isLoopEndNodeConfig = (config: unknown): config is z.infer<typeof LoopEndNodeConfigSchema> => {
  return LoopEndNodeConfigSchema.safeParse(config).success;
};