/**
 * The `edit` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * edit tool parameter Schema
 */
export const editSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    file_path: {
      type: "string",
      description: "The path of the file to edit (relative to the current workspace directory)",
    },
    old_string: {
      type: "string",
      description:
        "The exact string to search for and replace. This must match exactly in the file.",
    },
    new_string: {
      type: "string",
      description:
        "The new string to replace the old_string with. This will be inserted in place of old_string.",
    },
    mode: {
      type: "string",
      description:
        "Operation mode. 'safe' (default): requires unique match with fuzzy matching enabled, replaces first occurrence. 'batch': no uniqueness check, exact matching only, replaces all occurrences. Allowed values: 'safe', 'batch'.",
      enum: ["safe", "batch"],
    },
  },
  required: ["file_path", "old_string", "new_string"],
};
