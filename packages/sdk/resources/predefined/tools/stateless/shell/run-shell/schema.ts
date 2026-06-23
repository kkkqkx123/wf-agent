/**
 * Run-shell tool parameter Schema definition
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * run_shell tool parameter Schema
 */
export const runShellSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    command: {
      type: "string",
      description:
        "The shell command to execute. Quote file paths with spaces using double quotes.",
    },
    timeout: {
      type: "integer",
      description:
        "Optional: Timeout in seconds (default: 120, max: 600). Only applies to foreground commands.",
      default: 120,
    },
    shell_type: {
      type: "string",
      enum: ["bash", "zsh", "fish", "sh", "cmd", "powershell", "pwsh", "git-bash", "wsl"],
      description: "Shell type to use (default: platform-specific)",
    },
    cwd: {
      type: "string",
      description: "Working directory for the command (default: current directory)",
    },
    env: {
      type: "object",
      additionalProperties: { type: "string" },
      description: "Environment variables to set",
    },
  },
  required: ["command"],
};
