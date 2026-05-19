/**
 * Node Template Zod Schemas
 * Provides runtime validation schemas for Node Template configurations
 */

import { z } from "zod";
import type { NodeTemplate } from "./node-template.js";
import type { StaticNodeType } from "./node/static-node-types.js";

/**
 * Static Node Type Schema
 */
export const StaticNodeTypeSchema: z.ZodType<StaticNodeType> = z.enum([
  "START",
  "END",
  "VARIABLE",
  "FORK",
  "JOIN",
  "SYNC",
  "SUBGRAPH",
  "EMBED_GRAPH",
  "SCRIPT",
  "LLM",
  "ADD_TOOL",
  "TOOL_VISIBILITY",
  "USER_INTERACTION",
  "ROUTE",
  "CONTEXT_PROCESSOR",
  "LOOP_START",
  "LOOP_END",
  "AGENT_LOOP",
  "START_FROM_TRIGGER",
  "CONTINUE_FROM_TRIGGER",
]);

/**
 * Node Template Metadata Schema
 */
export const NodeTemplateMetadataSchema = z.object({
  category: z.string().optional(),
  tags: z.array(z.string()).optional(),
  customFields: z.record(z.string(), z.any()).optional(),
});

/**
 * Node Template Schema
 */
export const NodeTemplateSchema: z.ZodType<NodeTemplate> = z.object({
  name: z.string().min(1, "Node template name is required").max(100),
  type: StaticNodeTypeSchema,
  config: z.any(), // Node config is validated separately by NodeValidator
  description: z.string().max(500).optional(),
  metadata: NodeTemplateMetadataSchema.optional(),
  createdAt: z.number().int().min(0, "CreatedAt must be a non-negative integer"),
  updatedAt: z.number().int().min(0, "UpdatedAt must be a non-negative integer"),
});

/**
 * Type guard for NodeTemplate
 */
export function isNodeTemplate(value: unknown): value is NodeTemplate {
  return NodeTemplateSchema.safeParse(value).success;
}
