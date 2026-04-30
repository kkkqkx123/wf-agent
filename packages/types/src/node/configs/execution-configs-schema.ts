/**
 * Zod Schemas for Execution Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Script node configuration schema
 */
export const ScriptNodeConfigSchema = z.object({
  scriptName: z.string().min(1, "Script name is required"),
  scriptType: z.enum(["shell", "cmd", "powershell", "python", "javascript"], {
    message: "Script type must be one of: shell, cmd, powershell, python, javascript",
  }),
  risk: z.enum(["none", "low", "medium", "high"], {
    message: "Risk level must be one of: none, low, medium, high",
  }),
  timeout: z.number().positive("Timeout must be positive").optional(),
  retries: z.number().nonnegative("Retries must be non-negative").optional(),
  retryDelay: z.number().nonnegative("Retry delay must be non-negative").optional(),
  inline: z.boolean().optional(),
});

/**
 * LLM node configuration schema
 */
export const LLMNodeConfigSchema = z.object({
  profileId: z.string().min(1, "Profile ID is required"),
  prompt: z.string().optional(),
  promptTemplateId: z.string().optional(),
  promptTemplateVariables: z.record(z.string(), z.any()).optional(),
  parameters: z.record(z.string(), z.any()).optional(),
  maxToolCallsPerRequest: z
    .number()
    .min(1, "Max tool calls per request must be at least 1")
    .optional(),
});

/**
 * Add tool node configuration schema
 */
export const AddToolNodeConfigSchema = z.object({
  toolIds: z
    .array(z.string().min(1, "Tool ID must not be empty"))
    .min(1, "At least one tool ID is required"),
  descriptionTemplate: z.string().optional(),
  scope: z.enum(["GLOBAL", "WORKFLOW_EXECUTION", "LOCAL"], {
    message: "Scope must be one of: GLOBAL, WORKFLOW_EXECUTION, LOCAL",
  }).optional(),
  overwrite: z.boolean().optional(),
  metadata: z.record(z.string(), z.any()).optional(),
});

/**
 * Type guards for runtime type checking
 */
export const isScriptNodeConfig = (config: unknown): config is z.infer<typeof ScriptNodeConfigSchema> => {
  return ScriptNodeConfigSchema.safeParse(config).success;
};

export const isLLMNodeConfig = (config: unknown): config is z.infer<typeof LLMNodeConfigSchema> => {
  return LLMNodeConfigSchema.safeParse(config).success;
};

export const isAddToolNodeConfig = (config: unknown): config is z.infer<typeof AddToolNodeConfigSchema> => {
  return AddToolNodeConfigSchema.safeParse(config).success;
};