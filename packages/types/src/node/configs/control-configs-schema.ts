/**
 * Zod Schemas for Control Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Variable input mapping schema
 */
const variableInputSchema = z.object({
  externalName: z.string().min(1, "External name (parent variable) is required"),
  internalName: z.string().min(1, "Internal name (workflow variable) is required"),
  required: z.boolean().optional(),
  defaultValue: z.any().optional(),
  description: z.string().optional(),
});

/**
 * Variable output mapping schema
 */
const variableOutputSchema = z.object({
  internalName: z.string().min(1, "Internal name (workflow variable) is required"),
  externalName: z.string().min(1, "External name (parent variable) is required"),
  description: z.string().optional(),
});

/**
 * Start node configuration schema
 * Extended to support variable inputs/outputs and message context inputs/outputs declaration for subgraphs
 */
export const StartNodeConfigSchema = z.object({
  variableInputs: z.array(variableInputSchema).optional(),
  variableOutputs: z.array(variableOutputSchema).optional(),
  messageInputs: z.array(
    z.object({
      externalName: z.string().min(1, "External name is required"),
      internalName: z.string().min(1, "Internal name is required"),
      required: z.boolean().optional(),
      description: z.string().optional(),
    })
  ).optional(),
  messageOutputs: z.array(
    z.object({
      internalName: z.string().min(1, "Internal name is required"),
      externalName: z.string().min(1, "External name is required"),
      description: z.string().optional(),
    })
  ).optional(),
});

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