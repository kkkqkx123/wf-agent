/**
 * Backend-shell tool parameter Schema definition
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * backend_shell tool parameter Schema
 */
export const backendShellSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    command: {
      type: "string",
      description: "The shell command to execute in background",
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

/**
 * shell_output tool parameter Schema
 */
export const shellOutputSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    shell_id: {
      type: "string",
      description: "The ID of the background shell to retrieve output from",
    },
    filter_str: {
      type: "string",
      description: "Optional regular expression to filter the output lines",
    },
  },
  required: ["shell_id"],
};

/**
 * shell_kill tool parameters Schema
 */
export const shellKillSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    shell_id: {
      type: "string",
      description: "The ID of the background shell to terminate",
    },
  },
  required: ["shell_id"],
};
