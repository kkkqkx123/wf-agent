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
      type: "object",
      nullable: true,
      description:
        "Optional key-value pairs to pass as variables to the skill. These will be substituted into the skill content as template variables (e.g., {variableName}).",
      additionalProperties: true,
    },
  },
  required: ["skill"],
};
