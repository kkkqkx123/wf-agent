/**
 * Zod Schemas for Script Node Configuration
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
 * Type guard for runtime type checking
 */
export const isScriptNodeConfig = (config: unknown): config is z.infer<typeof ScriptNodeConfigSchema> => {
  return ScriptNodeConfigSchema.safeParse(config).success;
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