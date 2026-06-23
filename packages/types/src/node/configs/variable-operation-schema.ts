/**
 * Zod Schemas for Variable Operation Configuration
 * Provides runtime validation synchronized with TypeScript types
 */

import { z } from "zod";

/**
 * Filter expression schema
 */
export const FilterExpressionSchema = z.object({
  expression: z.string().min(1, "Filter expression cannot be empty"),
  valueType: z.enum(["array", "object"]).optional(),
});

/**
 * Variable aggregate operation schema
 */
export const VariableAggregateOperationSchema = z.object({
  operation: z.literal("aggregate"),
  sourceVariables: z
    .array(z.string().min(1))
    .min(2, "At least 2 source variables required for aggregation"),
  targetVariable: z.string().min(1, "Target variable name required"),
  aggregateMode: z.enum(["array", "object", "merge"]),
  keyMapping: z.record(z.string(), z.string()).optional(),
  filterExpression: FilterExpressionSchema.optional(),
  mergeStrategy: z.enum(["shallow", "deep"]).optional(),
});

/**
 * Variable transform operation schema
 */
export const VariableTransformOperationSchema = z.object({
  operation: z.literal("transform"),
  sourceVariable: z.string().min(1, "Source variable required"),
  targetVariable: z.string().min(1, "Target variable required"),
  transformExpression: z.string().min(1, "Transform expression required"),
  outputType: z.enum(["string", "number", "boolean", "array", "object"]).optional(),
});

/**
 * Variable batch update operation schema
 */
export const VariableBatchUpdateOperationSchema = z.object({
  operation: z.literal("batch-update"),
  updates: z
    .array(
      z.object({
        name: z.string().min(1, "Variable name required"),
        expression: z.string().min(1, "Expression required"),
        type: z.enum(["string", "number", "boolean", "array", "object"]).optional(),
        readonly: z.boolean().optional(),
      }),
    )
    .min(1, "At least one update required"),
});

/**
 * Union schema for all variable operations
 */
export const VariableOperationConfigSchema = z.union([
  VariableAggregateOperationSchema,
  VariableTransformOperationSchema,
  VariableBatchUpdateOperationSchema,
]);

/**
 * Variable operation output schema
 */
export const VariableOperationOutputSchema = z.object({
  operation: z.string(),
  modifiedVariables: z.array(
    z.object({
      name: z.string(),
      newValue: z.unknown(),
      type: z.string().optional(),
    }),
  ),
  executionTime: z.number().min(0),
  stats: z
    .object({
      sourceVariableCount: z.number().optional(),
      aggregatedItemCount: z.number().optional(),
    })
    .optional(),
});

/**
 * Type guard for variable operation config
 */
export const isVariableOperationConfig = (
  config: unknown,
): config is z.infer<typeof VariableOperationConfigSchema> => {
  return VariableOperationConfigSchema.safeParse(config).success;
};
