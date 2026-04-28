/**
 * The logic executed by the skill tool
 *
 * This tool loads and executes skills.
 * The handler returns the skill execution result.
 */

import type { ToolOutput } from "@wf-agent/types";

/**
 * Create the `skill` tool execution function
 */
export function createSkillHandler() {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { skill, args } = params as { skill: string; args?: string | null };

      if (!skill || typeof skill !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'skill' parameter",
        };
      }

      // Return a special result indicating skill execution
      // The workflow engine should handle this by loading and executing the skill
      return {
        success: true,
        content: `Skill: ${skill}${args ? ` with args: ${args}` : ""}`,
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
