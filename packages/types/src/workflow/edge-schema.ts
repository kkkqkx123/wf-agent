/**
 * Edge Zod Schemas
 * Provides runtime validation schemas for Edge configurations
 */

import { z } from "zod";
import type { Edge, EdgeType, EdgeMetadata } from "./edge.js";

/**
 * Edge Type Schema
 */
export const EdgeTypeSchema: z.ZodType<EdgeType> = z.enum(["DEFAULT", "CONDITIONAL"]);

/**
 * Edge Metadata Schema
 */
export const EdgeMetadataSchema: z.ZodType<EdgeMetadata> = z.object({
  tags: z.array(z.string()).optional(),
  customFields: z.record(z.string(), z.any()).optional(),
});

/**
 * Edge Schema
 */
export const EdgeSchema: z.ZodType<Edge> = z.object({
  id: z.string().min(1, "Edge ID is required"),
  sourceNodeId: z.string().min(1, "Source node ID is required"),
  targetNodeId: z.string().min(1, "Target node ID is required"),
  type: EdgeTypeSchema,
  condition: z.any().optional(), // Condition is validated separately
  label: z.string().optional(),
  description: z.string().optional(),
  weight: z.number().optional(),
  metadata: EdgeMetadataSchema.optional(),
});

/**
 * Type guard for Edge
 */
export function isEdge(value: unknown): value is Edge {
  return EdgeSchema.safeParse(value).success;
}
