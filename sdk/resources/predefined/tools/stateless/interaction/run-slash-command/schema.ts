/**
 * The `run_slash_command` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * run_slash_command tool parameter Schema
 */
export const runSlashCommandSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    command: {
      type: "string",
      description: "Name of the slash command to run (e.g., init, test, deploy)",
    },
    args: {
      type: "string",
      nullable: true,
      description: "Optional additional context or arguments for the command",
    },
  },
  required: ["command"],
};
