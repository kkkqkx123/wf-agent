/**
 * Zod Schemas for Loop Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

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
 * Description: Initializes loop iteration, supporting two loop modes
 *
 * Mode 1: Data-driven loop (providing dataSource)
 * - Iterates over a specified data collection (array, object, etc.)
 * - Automatically extracts the current value into the loop variable in each iteration
 * - Example: Iterate over [1,2,3], item = current value each time
 *
 * Mode 2: Count-driven loop (not providing dataSource)
 * - Loops a fixed number of times based only on maxIterations
 * - No loop variable, the loop body can maintain state itself
 * - Example: Check 10 times
 *
 * - Loop state (iteration count, index, etc.) is stored in loop-level scope and automatically managed with the scope lifecycle
 */
export const LoopStartNodeConfigSchema = z.object({
  loopId: z.string().min(1, "Loop ID is required"),
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