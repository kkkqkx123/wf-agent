/**
 * Run-shell Tool Description Definition
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const RUN_SHELL_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "run_shell",
  id: "run_shell",
  type: "STATELESS",
  category: "shell",
  description:
    "Execute shell commands in foreground or background. For terminal operations like git, npm, docker, etc. DO NOT use for file operations - use specialized tools.",
  parameters: [
    {
      name: "command",
      type: "string",
      required: true,
      description: "Shell command to execute. Quote file paths with spaces using double quotes.",
    },
    {
      name: "timeout",
      type: "integer",
      required: false,
      description: "Timeout in seconds (default: 120, max: 600) for foreground commands",
      defaultValue: 120,
    },
  ],
  tips: [
    'Quote file paths with spaces: cd "My Documents"',
    'Chain dependent commands with &&: git add . && git commit -m "msg"',
    "Use absolute paths instead of cd when possible",
    "DO NOT use for file operations - use specialized tools",
  ],
  examples: ["git status", "npm test", "pnpm build", "docker ps"],
};
