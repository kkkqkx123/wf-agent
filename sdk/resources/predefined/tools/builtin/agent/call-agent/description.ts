/**
 * Call Agent Tool Description
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

/**
 * Call agent tool description
 */
export const CALL_AGENT_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "call_agent",
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
    "Use subagents to maintain focus and apply specialized knowledge",
    "Provide clear and detailed prompts to the subagent for better results",
    "Subagents run in their own context but share the parent's execution hierarchy",
  ],
};
