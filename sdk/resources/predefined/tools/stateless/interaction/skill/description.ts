/**
 * Tool Description for `skill`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const SKILL_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "skill",
  id: "skill",
  type: "STATELESS",
  category: "code",
  description: `Load and execute a skill by name. Skills provide specialized instructions for common tasks like creating MCP servers or custom modes.

Use this tool when you need to follow specific procedures documented in a skill. Available skills are listed in the AVAILABLE SKILLS section of the system prompt.`,
  parameters: [
    {
      name: "skill",
      type: "string",
      required: true,
      description:
        "Name of the skill to load (e.g., create-mcp-server, create-mode). Must match a skill name from the available skills list.",
    },
    {
      name: "args",
      type: "string",
      required: false,
      description: "Optional context or arguments to pass to the skill",
    },
  ],
  tips: [
    "Skills provide specialized instructions for common tasks",
    "Check AVAILABLE SKILLS section for available skills",
    "Can pass additional arguments for context",
  ],
};
