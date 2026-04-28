/**
 * The `skill` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * skill tool parameter Schema
 */
export const skillSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    skill: {
      type: "string",
      description:
        "Name of the skill to load (e.g., create-mcp-server, create-mode). Must match a skill name from the available skills list.",
    },
    args: {
      type: "string",
      nullable: true,
      description: "Optional context or arguments to pass to the skill",
    },
  },
  required: ["skill"],
};
