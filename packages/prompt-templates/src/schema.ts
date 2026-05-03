/**
 * Prompt Template Zod Schemas
 * Provides runtime validation schemas for Prompt Template configurations
 */

import { z } from "zod";
import type { PromptTemplate, VariableDefinition } from "./types/template.js";

/**
 * Variable Definition Schema
 */
export const VariableDefinitionSchema: z.ZodType<VariableDefinition> = z.object({
  name: z.string().min(1, { message: "Variable name is required" }).max(100),
  type: z.enum(["string", "number", "boolean", "object", "array"], {
    message: "Variable type must be one of: string, number, boolean, object, array",
  }),
  required: z.boolean(),
  description: z.string().max(500).optional(),
  defaultValue: z.any().optional(),
});

/**
 * Prompt Template Schema
 */
export const PromptTemplateSchema: z.ZodType<PromptTemplate> = z.object({
  id: z.string().min(1, { message: "Template ID is required" }).max(100),
  name: z.string().min(1, { message: "Template name is required" }).max(200),
  description: z.string().max(1000),
  category: z.enum(
    ["system", "rules", "user-command", "tools", "composite", "fragments", "dynamic"],
    {
      message:
        "Category must be one of: system, rules, user-command, tools, composite, fragments, dynamic",
    },
  ),
  content: z.string().min(1, { message: "Template content is required" }),
  variables: z.array(VariableDefinitionSchema).optional(),
  fragments: z.array(z.string().min(1)).optional(),
});

/**
 * Type guard for runtime type checking
 */
export function isPromptTemplate(value: unknown): value is PromptTemplate {
  return PromptTemplateSchema.safeParse(value).success;
}
