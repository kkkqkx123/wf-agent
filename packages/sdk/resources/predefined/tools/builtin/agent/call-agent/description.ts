/**
 * Call Agent Tool Description
 */

import type { ToolDescriptionData } from "@wf-agent/types";
import type { AgentInfo } from "./handler.js";

/**
 * Call agent tool description
 */
export const CALL_AGENT_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "builtin_call_agent",
  type: "STATELESS",
  category: "agent",
  description:
    "Invoke a specialized subagent to handle a specific task. Use this when you need to delegate a complex or specialized sub-task (like code review, testing, or documentation) to an agent with a specific profile and toolset.",
  parameters: [
    {
      name: "agentProfileId",
      type: "string",
      required: true,
      description: "The ID of the subagent profile to execute",
    },
    {
      name: "prompt",
      type: "string",
      required: true,
      description: "The task description or prompt for the subagent",
    },
    {
      name: "input",
      type: "object",
      required: false,
      description: "Additional context or variables to pass to the subagent",
    },
    {
      name: "waitForCompletion",
      type: "boolean",
      required: false,
      description: "Whether to wait for the agent to complete (default: true)",
      defaultValue: true,
    },
  ],
  tips: [
    "Delegate specialized sub-tasks to appropriate agent profiles",
    "Provide a clear and detailed prompt to the subagent",
    "The subagent has its own toolset and system prompt",
    "Use this to parallelize complex workflows across multiple agents",
  ],
};

/**
 * Generate a dynamic call_agent tool description with available agent profiles.
 *
 * Injects available agent profile IDs and descriptions so the LLM can
 * discover which agents are available and what they do.
 *
 * @param agents - Available agent profiles to inject into the description
 * @returns A new ToolDescriptionData with agent profile information in tips
 */
export function generateCallAgentDescription(agents: AgentInfo[]): ToolDescriptionData {
  if (!agents || agents.length === 0) {
    return CALL_AGENT_TOOL_DESCRIPTION;
  }

  const agentListLines = agents.map(a => `  - ${a.id}: ${a.description || a.name}`);

  const tip = `Available agent profiles:\n${agentListLines.join("\n")}`;

  return {
    ...CALL_AGENT_TOOL_DESCRIPTION,
    tips: [tip, ...(CALL_AGENT_TOOL_DESCRIPTION.tips ?? [])],
  };
}
