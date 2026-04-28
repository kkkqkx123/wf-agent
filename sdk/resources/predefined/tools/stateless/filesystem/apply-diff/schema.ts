/**
 * The `apply_diff` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * apply_diff tool parameter Schema
 */
export const applyDiffSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    path: {
      type: "string",
      description: "The path of the file to edit (relative to the current workspace directory)",
    },
    diff: {
      type: "string",
      description:
        "The diff content to apply. Should follow unified diff format with context lines to uniquely identify the changes.",
    },
  },
  required: ["path", "diff"],
};
