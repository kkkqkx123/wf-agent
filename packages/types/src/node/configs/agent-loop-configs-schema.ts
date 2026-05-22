/**
 * Agent Loop Node Configuration Zod Schema
 *
 * Provides runtime validation and type guards for AgentLoopNodeConfig.
 */

import { z } from "zod";
import type { AgentLoopNodeConfig } from "./agent-loop-configs.js";
import { WorkflowDataInputSchema } from "../../workflow/boundary-config-schema.js";

/**
 * AgentToolConfig Schema (matches the AgentToolConfig interface)
 */
const AgentToolConfigSchema = z.object({
  tools: z.array(z.string()),
  requireApproval: z.array(z.string()).optional(),
}).optional();

/**
 * Inline Config Schema
 */
const InlineConfigSchema = z.object({
  profileId: z.string(),
  maxIterations: z.number().int().positive().optional(),
  availableTools: AgentToolConfigSchema,
  initialContextId: z.string().optional(),
  workingContext: z.string().optional(),
  dataInputs: z.array(WorkflowDataInputSchema).optional(),
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
