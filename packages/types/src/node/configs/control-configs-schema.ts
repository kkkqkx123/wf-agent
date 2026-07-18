/**
 * Zod Schemas for Control Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 *
 * Note: START and END nodes use WorkflowStartConfig and WorkflowEndConfig from
 * workflow/boundary-config.ts. Their validation is handled directly in validators.
 */

import { z } from "zod";

const ExpressionConditionSchema = z.object({
  type: z.literal("expression"),
  expression: z.string().min(1, "Expression condition requires a non-empty expression string"),
  metadata: z.any().optional(),
});

const PredicateConditionSchema = z.object({
  type: z.literal("predicate"),
  predicateType: z.enum(["isEmpty", "isNotEmpty", "isNull", "isNotNull", "isTrue", "isFalse"], {
    message: "predicateType must be one of: isEmpty, isNotEmpty, isNull, isNotNull, isTrue, isFalse",
  }),
  variable: z.string().min(1, "Predicate condition requires a non-empty variable name"),
  metadata: z.any().optional(),
});

const ScriptConditionSchema = z.object({
  type: z.literal("script"),
  script: z.string().min(1, "Script condition requires a non-empty script string"),
  metadata: z.any().optional(),
});

const SchemaConditionSchema = z.object({
  type: z.literal("schema"),
  variable: z.string().min(1, "Schema condition requires a non-empty variable name"),
  schema: z.record(z.string(), z.unknown()),
  metadata: z.any().optional(),
});

const ConditionSchema = z.discriminatedUnion("type", [
  ExpressionConditionSchema,
  PredicateConditionSchema,
  ScriptConditionSchema,
  SchemaConditionSchema,
]);

/**
 * Route node configuration schema
 */
export const RouteNodeConfigSchema = z.object({
  routes: z
    .array(
      z.object({
        condition: ConditionSchema,
        targetNodeId: z.string().min(1, "Target node ID is required"),
        priority: z.number().optional(),
      }),
    )
    .min(1, "Routes array cannot be empty"),
  defaultTargetNodeId: z.string().optional(),
});

/**
 * Type guard for runtime type checking
 */
export const isRouteNodeConfig = (config: unknown): config is z.infer<typeof RouteNodeConfigSchema> => {
  return RouteNodeConfigSchema.safeParse(config).success;
};