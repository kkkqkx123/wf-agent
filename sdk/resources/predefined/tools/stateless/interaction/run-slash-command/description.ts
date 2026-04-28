/**
 * Tool Description for `run_slash_command`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const RUN_SLASH_COMMAND_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "run_slash_command",
  id: "run_slash_command",
  type: "STATELESS",
  category: "code",
  description: `Execute a slash command to get specific instructions or content. Slash commands are predefined templates that provide detailed guidance for common tasks.`,
  parameters: [
    {
      name: "command",
      type: "string",
      required: true,
      description: "Name of the slash command to run (e.g., init, test, deploy)",
    },
    {
      name: "args",
      type: "string",
      required: false,
      description: "Optional additional context or arguments for the command",
    },
  ],
  tips: [
    "Slash commands provide predefined templates for common tasks",
    "Use to get specific instructions or guidance",
    "Can pass additional arguments for context",
  ],
};
