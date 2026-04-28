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
    replace_all: {
      type: "boolean",
      description:
        "If true, replace all occurrences of old_string in the file. If false or omitted, only replace the first occurrence (default: false).",
    },
    require_unique: {
      type: "boolean",
      description:
        "If true, the old_string must appear exactly once in the file. If it appears multiple times, the operation will fail. This is useful to prevent accidental replacements in multiple locations (default: false).",
    },
  },
  required: ["file_path", "old_string", "new_string"],
};
