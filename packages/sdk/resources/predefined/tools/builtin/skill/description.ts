/**
 * Tool Description for `skill`
 */

import type { ToolDescriptionData } from "@wf-agent/types";

export const SKILL_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "skill",
  type: "STATELESS",
  category: "code",
  description: `Load and execute a skill by name. Skills provide specialized instructions and context for common tasks like creating MCP servers or custom modes.

Use this tool when you need to follow specific procedures documented in a skill. Available skills are listed in the "Available Skills" section of the system prompt.

The skill content will be loaded and returned as the tool result. You can pass optional variables via the "args" parameter for template substitution within the skill content.`,
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
      type: "object",
      required: false,
      description:
        "Optional key-value pairs to pass as variables to the skill. These will be substituted into the skill content as template variables (e.g., {variableName}).",
    },
  ],
  tips: [
    "Skills provide specialized instructions for common tasks",
    "Check the Available Skills section for available skills",
    "Pass additional arguments as key-value pairs for variable substitution",
    "Skills are loaded on demand - only call when you need the specific instructions",
  ],
};
