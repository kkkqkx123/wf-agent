import { z } from "zod";
import type { PromptTemplate, PromptVariableDefinition } from "./prompt-template.js";

export const PromptVariableDefinitionSchema: z.ZodType<PromptVariableDefinition> = z.object({
  name: z.string().min(1).max(100),
  type: z.enum(["string", "number", "boolean", "object", "array"]),
  required: z.boolean(),
  description: z.string().max(500).optional(),
  defaultValue: z.any().optional(),
});

export const PromptTemplateSchema: z.ZodType<PromptTemplate> = z.object({
  id: z.string().min(1).max(100),
  name: z.string().min(1).max(200),
  description: z.string().max(1000),
  category: z.enum(
    ["system", "rules", "user-command", "tools", "composite", "fragments", "dynamic"],
  ),
  content: z.string().min(1),
  variables: z.array(PromptVariableDefinitionSchema).optional(),
  fragments: z.array(z.string().min(1)).optional(),
});

export function isPromptTemplate(value: unknown): value is PromptTemplate {
  return PromptTemplateSchema.safeParse(value).success;
}