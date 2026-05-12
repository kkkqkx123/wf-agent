/**
 * Zod Schemas for Trigger-based Subworkflow Node Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";

/**
 * Start from trigger node configuration schema
 * Supports message context inputs declaration for clear interface contracts
 */
export const StartFromTriggerNodeConfigSchema = z.object({
  messageInputs: z.array(
    z.object({
      externalName: z.string().min(1, "External name is required"),
      internalName: z.string().min(1, "Internal name is required"),
      required: z.boolean().optional(),
      description: z.string().optional(),
    })
  ).optional(),
});

/**
 * Continue from trigger node configuration schema
 * Supports message context outputs and variable callback configuration
 */
export const ContinueFromTriggerNodeConfigSchema = z.object({
  messageOutputs: z.array(
    z.object({
      internalName: z.string().min(1, "Internal name is required"),
      externalName: z.string().min(1, "External name is required"),
      description: z.string().optional(),
    })
  ).optional(),
  
  variableCallback: z.object({
    includeVariables: z.array(z.string()).optional(),
    includeAll: z.boolean().optional(),
  }).optional(),
});

/**
 * Type guards for runtime type checking
 */
export const isStartFromTriggerNodeConfig = (config: unknown): config is z.infer<typeof StartFromTriggerNodeConfigSchema> => {
  return StartFromTriggerNodeConfigSchema.safeParse(config).success;
};

export const isContinueFromTriggerNodeConfig = (config: unknown): config is z.infer<typeof ContinueFromTriggerNodeConfigSchema> => {
  return ContinueFromTriggerNodeConfigSchema.safeParse(config).success;
};
