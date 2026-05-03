/**
 * Agent Loop Configuration File Zod Schemas
 * 
 * Provides validation schemas for AgentLoopConfigFile (file-based configuration).
 * These schemas validate TOML/JSON configuration files before transformation to runtime config.
 */

import { z } from "zod";

/**
 * Agent Hook Configuration File Schema
 */
export const AgentHookConfigFileSchema = z.object({
  hookType: z.string(), // AgentHookType - using z.string() as enum validation happens at runtime
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
 */
export const AgentLoopCheckpointConfigSchema = z.object({
  createOnEnd: z.boolean().optional(),
  createOnError: z.boolean().optional(),
  createOnIteration: z.boolean().optional(),
});

/**
 * Metadata Schema
 */
export const AgentLoopMetadataSchema = z.record(z.string(), z.unknown());

/**
 * Agent Loop Config File Schema
 * Validates the entire Agent Loop configuration file structure
 */
export const AgentLoopConfigFileSchema = z.object({
  id: z.string().min(1, "Agent loop ID is required"),
  name: z.string().optional(),
  description: z.string().optional(),
  version: z.string().optional(),
  profileId: z.string().optional(),
  systemPrompt: z.string().optional(),
  systemPromptTemplate: z.string().optional(),
  maxIterations: z.number().int().optional(),
  initialMessages: z.array(z.any()).optional(), // Message[] - using z.any() as Message schema not yet available
  tools: z.array(z.string()).optional(),
  stream: z.boolean().optional(),
  checkpoint: AgentLoopCheckpointConfigSchema.optional(),
  hooks: z.array(AgentHookConfigFileSchema).optional(),
  triggers: z.array(AgentTriggerConfigFileSchema).optional(),
  metadata: AgentLoopMetadataSchema.optional(),
});
