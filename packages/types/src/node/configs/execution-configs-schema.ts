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
  risk: z.enum(["none", "low", "medium", "high"], {
    message: "Risk level must be one of: none, low, medium, high",
  }),
  inline: z.boolean().optional(),
  template: z.string().optional(),
  executor: z.object({
    mode: z.enum(["direct", "shared", "pty"], {
      message: "Executor mode must be one of: direct, shared, pty",
    }).optional(),
    shell: z.enum(["powershell", "bash", "cmd", "auto"], {
      message: "Shell type must be one of: powershell, bash, cmd, auto",
    }).optional(),
    cwd: z.string().optional(),
    environment: z.record(z.string(), z.string()).optional(),
  }).optional(),
  flowId: z.string().optional(),
  arguments: z.record(z.string(), z.unknown()).optional(),
});

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
});

/**
 * AddToolNodeConfigSchema has been removed.
 * 
 * ADD_TOOL node type is deprecated.
 * Tool visibility should be managed via TOOL_VISIBILITY nodes
 * and context operations (CONTEXT_PROCESSOR nodes with ADD_MESSAGE operation).
 */

/**
 * Type guards for runtime type checking
 */
export const isScriptNodeConfig = (config: unknown): config is z.infer<typeof ScriptNodeConfigSchema> => {
  return ScriptNodeConfigSchema.safeParse(config).success;
};

export const isLLMNodeConfig = (config: unknown): config is z.infer<typeof LLMNodeConfigSchema> => {
  return LLMNodeConfigSchema.safeParse(config).success;
};

/**
 * Interactive script node configuration schema
 */
export const InteractiveScriptNodeConfigSchema = z.object({
  scriptName: z.string().min(1, "Script name is required"),
  risk: z.enum(["none", "low", "medium", "high"], {
    message: "Risk level must be one of: none, low, medium, high",
  }),
  executor: z.object({
    mode: z.enum(["direct", "shared", "pty"], {
      message: "Executor mode must be one of: direct, shared, pty",
    }).optional(),
    shell: z.enum(["powershell", "bash", "cmd", "auto"], {
      message: "Shell type must be one of: powershell, bash, cmd, auto",
    }).optional(),
    cwd: z.string().optional(),
    environment: z.record(z.string(), z.string()).optional(),
  }).optional(),
  flowId: z.string().optional(),
  interactionMode: z.enum(["blocking", "llm-assisted", "hybrid"], {
    message: "Interaction mode must be one of: blocking, llm-assisted, hybrid",
  }).optional(),
  promptPatterns: z.array(z.string()).optional(),
  maxRounds: z.number().int().positive().optional(),
  roundTimeout: z.number().positive().optional(),
});

export const isInteractiveScriptNodeConfig = (config: unknown): config is z.infer<typeof InteractiveScriptNodeConfigSchema> => {
  return InteractiveScriptNodeConfigSchema.safeParse(config).success;
};