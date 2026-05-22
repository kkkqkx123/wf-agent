/**
 * Agent Loop Node Configuration Zod Schema
 *
 * Provides runtime validation and type guards for AgentLoopNodeConfig.
 */

import { z } from "zod";
import type { AgentLoopNodeConfig } from "./agent-loop-configs.js";
import { WorkflowDataInputSchema, WorkflowMessageInputSchema, WorkflowMessageOutputSchema } from "../../workflow/boundary-config-schema.js";

/**
 * AgentToolConfig Schema (matches the AgentToolConfig interface)
 */
const AgentToolConfigSchema = z.object({
  tools: z.array(z.string()),
  requireApproval: z.array(z.string()).optional(),
}).optional();

/**
 * Inline Config Schema
 *
 * profileId is optional (can come from agentLoopId definition).
 * When inlineConfig is used without agentLoopId, profileId is required
 * (validated by the parent schema's refine).
 */
const InlineConfigSchema = z.object({
  profileId: z.string().optional(),
  maxIterations: z.number().int().positive().optional(),
  availableTools: AgentToolConfigSchema,
  workingContext: z.string().optional(),
  dataInputs: z.array(WorkflowDataInputSchema).optional(),
  messageInputs: z.array(WorkflowMessageInputSchema).optional(),
  messageOutputs: z.array(WorkflowMessageOutputSchema).optional(),
});

/**
 * Agent Loop Node Configuration Schema
 */
export const AgentLoopNodeConfigSchema = z.object({
  agentLoopId: z.string().optional(),
  inlineConfig: InlineConfigSchema.optional(),
}).refine(
  (data) => data.agentLoopId !== undefined || data.inlineConfig !== undefined,
  {
    message: "Either agentLoopId or inlineConfig must be provided",
    path: ["agentLoopId"],
  },
).refine(
  (data) => {
    // When inlineConfig is used standalone (no agentLoopId), profileId is required
    if (!data.agentLoopId && data.inlineConfig) {
      return data.inlineConfig.profileId !== undefined && data.inlineConfig.profileId.length > 0;
    }
    return true;
  },
  {
    message: "profileId is required in inlineConfig when agentLoopId is not provided",
    path: ["inlineConfig", "profileId"],
  },
) satisfies z.ZodType<AgentLoopNodeConfig>;

/**
 * Type guard for AgentLoopNodeConfig
 */
export function isAgentLoopNodeConfig(value: unknown): value is AgentLoopNodeConfig {
  return AgentLoopNodeConfigSchema.safeParse(value).success;
}
