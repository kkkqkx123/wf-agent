/**
 * Zod Schemas for Control Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Start node configuration schema (must be an empty object)
 */
export const StartNodeConfigSchema = z.object({}).strict();

/**
 * End node configuration schema (must be an empty object)
 */
export const EndNodeConfigSchema = z.object({}).strict();

/**
 * Route node configuration schema
 */
export const RouteNodeConfigSchema = z.object({
  routes: z
    .array(
      z.object({
        condition: z.object({
          expression: z.string().min(1, "Route condition expression is required"),
          metadata: z.any().optional(),
        }),
        targetNodeId: z.string().min(1, "Target node ID is required"),
        priority: z.number().optional(),
      }),
    )
    .min(1, "Routes array cannot be empty"),
  defaultTargetNodeId: z.string().optional(),
});

/**
 * Type guards for runtime type checking
 */
export const isStartNodeConfig = (config: unknown): config is z.infer<typeof StartNodeConfigSchema> => {
  return StartNodeConfigSchema.safeParse(config).success;
};

export const isEndNodeConfig = (config: unknown): config is z.infer<typeof EndNodeConfigSchema> => {
  return EndNodeConfigSchema.safeParse(config).success;
};

export const isRouteNodeConfig = (config: unknown): config is z.infer<typeof RouteNodeConfigSchema> => {
  return RouteNodeConfigSchema.safeParse(config).success;
};