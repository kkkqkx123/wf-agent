/**
 * Zod Schemas for Control Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 * 
 * Note: START and END nodes use WorkflowStartConfig and WorkflowEndConfig from
 * workflow/boundary-config.ts. Their validation is handled directly in validators.
 */

import { z } from "zod";

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
 * Type guard for runtime type checking
 */
export const isRouteNodeConfig = (config: unknown): config is z.infer<typeof RouteNodeConfigSchema> => {
  return RouteNodeConfigSchema.safeParse(config).success;
};