/**
 * Agent Loop Node Configuration Zod Schema
 *
 * Provides runtime validation and type guards for AgentLoopNodeConfig.
 */

import { z } from "zod";
import type { AgentLoopNodeConfig } from "./agent-loop-configs.js";

/**
 * AvailableTools Schema (matches the AvailableTools interface)
 */
const AvailableToolsSchema = z.object({
  initial: z.array(z.string()),
  dynamic: z.set(z.string()).optional(),
  filterMode: z.enum(['none', 'allowlist', 'blocklist']).optional(),
  allowList: z.array(z.string()).optional(),
  blockList: z.array(z.string()).optional(),
}).optional();

/**
 * Inline Config Schema
 */
const InlineConfigSchema = z.object({
  profileId: z.string(),
  maxIterations: z.number().int().positive().optional(),
  availableTools: AvailableToolsSchema,
  systemPrompt: z.string().optional(),
  systemPromptTemplateId: z.string().optional(),
  systemPromptTemplateVariables: z.record(z.string(), z.unknown()).optional(),
});

/**
 * Agent Loop Node Configuration Schema
 */
export const AgentLoopNodeConfigSchema = z.object({
  agentLoopId: z.string().optional(),
  inlineConfig: InlineConfigSchema.optional(),
}) satisfies z.ZodType<AgentLoopNodeConfig>;

/**
 * Type guard for AgentLoopNodeConfig
 */
export function isAgentLoopNodeConfig(value: unknown): value is AgentLoopNodeConfig {
  return AgentLoopNodeConfigSchema.safeParse(value).success;
}
