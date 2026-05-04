/**
 * Call Agent Tool Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * Call agent parameters schema
 */
export const callAgentSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    agentProfileId: {
      type: "string",
      description: "The ID of the subagent profile to execute",
    },
    prompt: {
      type: "string",
      description: "The task description or prompt for the subagent",
    },
    input: {
      type: "object",
      description: "Additional context or variables to pass to the subagent",
      additionalProperties: true,
    },
    waitForCompletion: {
      type: "boolean",
      description: "Whether to wait for the agent to complete (default: true)",
    },
  },
  required: ["agentProfileId", "prompt"],
};
