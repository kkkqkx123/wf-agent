/**
 * The logic executed by the skill tool
 *
 * This tool loads and executes skills.
 * The handler returns the skill execution result.
 */

import type { ToolOutput } from "@wf-agent/types";

/**
 * Configuration for the skill tool handler
 */
export interface SkillHandlerConfig {
  loader: {
    loadContent: (name: string, variables?: Record<string, unknown>) => Promise<string>;
  };
}

/**
 * Create the `skill` tool execution function
 *
 * @param config Optional configuration with a loader that provides skill content loading.
 *               If not provided, the handler returns an error indicating the skill system
 *               is not available.
 */
export function createSkillHandler(config?: SkillHandlerConfig) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { skill, args } = params as {
        skill: string;
        args?: Record<string, unknown> | null;
      };

      if (!skill || typeof skill !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'skill' parameter",
        };
      }

      if (!config) {
        return {
          success: false,
          content: "",
          error:
            "Skill system is not available. Please configure skill paths before using skills.",
        };
      }

      const variables = args || undefined;
      const content = await config.loader.loadContent(skill, variables);

      return {
        success: true,
        content,
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
