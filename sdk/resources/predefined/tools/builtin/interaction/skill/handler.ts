/**
 * The logic executed by the skill tool
 *
 * This tool loads and executes skills.
 * The handler returns the skill execution result.
 */

import type { ToolOutput } from "@wf-agent/types";

/**
 * Metadata for an available skill
 */
export interface SkillInfo {
  name: string;
  description: string;
}

/**
 * Configuration for the skill tool handler
 */
export interface SkillHandlerConfig {
  loader: {
    /** Get the list of all available skills with their metadata. */
    getAvailableSkills: () => SkillInfo[];
    /** Check whether a skill with the given name exists. */
    hasSkill: (name: string) => boolean;
    /** Load the full content of a skill by name. */
    loadContent: (name: string, variables?: Record<string, unknown>) => Promise<string>;
  };
}

/**
 * Format the list of available skills into a human-readable string.
 */
function formatAvailableSkills(skills: SkillInfo[]): string {
  if (skills.length === 0) {
    return "(no skills available)";
  }
  return skills.map(s => `  - ${s.name}: ${s.description}`).join("\n");
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
          error: "Missing or invalid 'skill' parameter. Please provide a valid skill name.",
        };
      }

      if (!config) {
        return {
          success: false,
          content: "",
          error: "Skill system is not available. Please configure skill paths before using skills.",
        };
      }

      // Validate skill existence (matching Roo Code's pattern)
      if (!config.loader.hasSkill(skill)) {
        const available = config.loader.getAvailableSkills();
        const availableList = formatAvailableSkills(available);
        return {
          success: false,
          content: "",
          error:
            `Skill '${skill}' not found.\n\nAvailable skills:\n${availableList}\n\n` +
            `Use the 'skill' tool with one of the available skill names listed above. ` +
            `Each skill provides specialized instructions for specific tasks.`,
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
