/**
 * Agent Loop Configuration Zod Schemas
 * 
 * Provides validation schemas for AgentLoopDefinition (file-based configuration).
 * These schemas validate TOML/JSON configuration files before transformation to runtime config.
 * 
 * Design Principle:
 * - Schemas live in types package alongside type definitions
 * - Validators and business logic remain in SDK's config module
 */

import { z } from "zod";
import { ToolCallFormatConfigSchema } from "../llm/tool-call-format.js";

/**
 * Agent Hook Configuration File Schema
 * Validates AgentHookStatic structure from file configuration
 */
export const AgentHookConfigFileSchema = z.object({
  hookType: z.enum([
    "BEFORE_ITERATION",
    "AFTER_ITERATION",
    "BEFORE_TOOL_CALL",
    "AFTER_TOOL_CALL",
    "BEFORE_LLM_CALL",
    "AFTER_LLM_CALL",
  ]),
  condition: z.string().optional(),
  eventName: z.string(),
  eventPayload: z.record(z.string(), z.unknown()).optional(),
  enabled: z.boolean().optional(),
  weight: z.number().optional(),
  createCheckpoint: z.boolean().optional(),
  checkpointDescription: z.string().optional(),
});

/**
 * Agent Trigger Action Schema
 */
export const AgentTriggerActionSchema = z.object({
  type: z.enum(["pause", "stop", "checkpoint", "custom"]),
  config: z.record(z.string(), z.unknown()).optional(),
});

/**
 * Agent Trigger Configuration File Schema
 * Validates AgentTriggerStatic structure from file configuration
 */
export const AgentTriggerConfigFileSchema = z.object({
  id: z.string(),
  type: z.enum(["event", "condition", "schedule"]),
  condition: z.string().optional(),
  eventName: z.string().optional(),
  enabled: z.boolean().optional(),
  action: AgentTriggerActionSchema,
});

/**
 * Checkpoint Configuration Schema
 * Validates AgentCheckpointConfig structure
 */
export const AgentLoopCheckpointConfigSchema = z.object({
  createOnEnd: z.boolean().optional(),
  createOnError: z.boolean().optional(),
  createOnIteration: z.boolean().optional(),
});

/**
 * Metadata Schema
 * Validates AgentLoopMetadata structure
 */
export const AgentLoopMetadataSchema = z.record(z.string(), z.unknown());

/**
 * Agent Tool Configuration Schema
 * Matches AgentToolConfig interface: tools + requireApproval
 */
export const AgentToolConfigSchema = z.object({
  tools: z.array(z.string()),
  requireApproval: z.array(z.string()).optional(),
});

/**
 * Agent Loop Definition Schema
 * Validates the entire Agent Loop configuration file structure
 * This schema corresponds to AgentLoopDefinition type
 */
export const AgentLoopDefinitionSchema = z.object({
  id: z.string().min(1, "Agent loop ID is required"),
  name: z.string().optional(),
  description: z.string().optional(),
  version: z.string().optional(),
  profileId: z.string().optional(),
  systemPrompt: z.string().optional(),
  systemPromptTemplateId: z.string().optional(),
  systemPromptTemplateVariables: z.record(z.string(), z.unknown()).optional(),
  maxIterations: z.number().int().optional(),
  initialMessages: z.array(z.any()).optional(), // Message[] - using z.any() as Message schema not yet available
  
  // Tool configuration (Agent-specific)
  availableTools: AgentToolConfigSchema.optional(),
  
  stream: z.boolean().optional(),
  checkpoint: AgentLoopCheckpointConfigSchema.optional(),
  toolCallFormat: ToolCallFormatConfigSchema.optional(),
  hooks: z.array(AgentHookConfigFileSchema).optional(),
  triggers: z.array(AgentTriggerConfigFileSchema).optional(),
  metadata: AgentLoopMetadataSchema.optional(),
  
  createdAt: z.number().optional(),
  updatedAt: z.number().optional(),
});

/**
 * Type Guards
 */

/**
 * Check if a value is a valid AgentLoopDefinition
 */
export function isAgentLoopDefinition(value: unknown): value is z.infer<typeof AgentLoopDefinitionSchema> {
  return AgentLoopDefinitionSchema.safeParse(value).success;
}

/**
 * Check if a value is a valid AgentHookStatic
 */
export function isAgentHookStatic(value: unknown): value is z.infer<typeof AgentHookConfigFileSchema> {
  return AgentHookConfigFileSchema.safeParse(value).success;
}

/**
 * Check if a value is a valid AgentTriggerStatic
 */
export function isAgentTriggerStatic(value: unknown): value is z.infer<typeof AgentTriggerConfigFileSchema> {
  return AgentTriggerConfigFileSchema.safeParse(value).success;
}
