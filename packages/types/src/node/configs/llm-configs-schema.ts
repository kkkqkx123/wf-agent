/**
 * Zod Schemas for LLM Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import { ToolCallFormatConfigSchema } from "../../llm/tool-call-format.js";

/**
 * LLM node configuration schema
 */
export const LLMNodeConfigSchema = z.object({
  profileId: z.string().min(1, "Profile ID is required"),
  contextId: z.string().optional(),
  outputContext: z.string().optional(),
  parameters: z.record(z.string(), z.any()).optional(),
  maxToolCallsPerRequest: z
    .number()
    .min(1, "Max tool calls per request must be at least 1")
    .optional(),
  toolCallFormat: ToolCallFormatConfigSchema.optional(),
});

/**
 * Type guard for runtime type checking
 */
export const isLLMNodeConfig = (config: unknown): config is z.infer<typeof LLMNodeConfigSchema> => {
  return LLMNodeConfigSchema.safeParse(config).success;
};