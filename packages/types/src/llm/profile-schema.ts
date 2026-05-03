/**
 * LLM Profile Zod Schemas
 * Provides runtime validation schemas for LLM Profile configurations
 */

import { z } from "zod";
import type { LLMProfile } from "./profile.js";
import type { LLMProvider } from "./state.js";
import { ToolCallFormatConfigSchema } from "./tool-call-format.js";

/**
 * LLM Provider Schema
 */
export const LLMProviderSchema: z.ZodType<LLMProvider> = z.enum([
  "OPENAI_CHAT",
  "OPENAI_RESPONSE",
  "ANTHROPIC",
  "GEMINI_NATIVE",
  "GEMINI_OPENAI",
  "HUMAN_RELAY",
]);

/**
 * LLM Profile Schema
 */
export const LLMProfileSchema: z.ZodType<LLMProfile> = z.object({
  id: z.string().min(1, { message: "Profile ID is required" }).max(100),
  name: z.string().min(1, { message: "Profile name is required" }).max(200),
  provider: LLMProviderSchema,
  model: z.string().min(1, { message: "Model name is required" }).max(100),
  apiKey: z.string().min(1, { message: "API key is required" }),
  baseUrl: z.string().min(1).optional(),
  parameters: z.record(z.string(), z.any()),
  headers: z.record(z.string(), z.string()).optional(),
  timeout: z.number().min(0, { message: "Timeout must be non-negative" }).int().optional(),
  maxRetries: z.number().min(0, { message: "Max retries must be non-negative" }).int().optional(),
  retryDelay: z.number().min(0, { message: "Retry delay must be non-negative" }).int().optional(),
  metadata: z.record(z.string(), z.any()).optional(),
  toolCallFormat: ToolCallFormatConfigSchema.optional(),
});

/**
 * Type guard for runtime type checking
 */
export function isLLMProfile(value: unknown): value is LLMProfile {
  return LLMProfileSchema.safeParse(value).success;
}
