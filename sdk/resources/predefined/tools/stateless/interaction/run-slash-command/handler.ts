/**
 * The logic executed by the run_slash_command tool
 *
 * This tool executes predefined slash commands.
 * The handler returns the command result or instructions.
 */

import type { ToolOutput } from "@wf-agent/types";

/**
 * Create the `run_slash_command` tool execution function
 */
export function createRunSlashCommandHandler() {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { command, args } = params as { command: string; args?: string | null };

      if (!command || typeof command !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'command' parameter",
        };
      }

      // Return a special result indicating slash command execution
      // The workflow engine should handle this by executing the slash command
      return {
        success: true,
        content: `Slash command: /${command}${args ? ` ${args}` : ""}`,
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  };
}
