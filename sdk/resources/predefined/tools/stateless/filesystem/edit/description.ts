/**
 * Tool Description for `edit`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const EDIT_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "edit",
  id: "edit",
  type: "STATELESS",
  category: "filesystem",
  description: `Edit a file by searching for and replacing a specific string. This tool performs exact string matching and replacement.

This tool performs exact matching. Ensure old_string matches exactly, including whitespace and case sensitivity. For more complex edits, consider using apply_patch or write_to_file.`,
  parameters: [
    {
      name: "file_path",
      type: "string",
      required: true,
      description: "The path of the file to edit (relative to the current workspace directory)",
    },
    {
      name: "old_string",
      type: "string",
      required: true,
      description:
        "The exact string to search for and replace. This must match exactly in the file.",
    },
    {
      name: "new_string",
      type: "string",
      required: true,
      description:
        "The new string to replace the old_string with. This will be inserted in place of old_string.",
    },
    {
      name: "replace_all",
      type: "boolean",
      required: false,
      description:
        "If true, replace all occurrences of old_string in the file. If false or omitted, only replace the first occurrence (default: false).",
      defaultValue: false,
    },
    {
      name: "require_unique",
      type: "boolean",
      required: false,
      description:
        "If true, the old_string must appear exactly once in the file. If it appears multiple times, the operation will fail. This is useful to prevent accidental replacements in multiple locations (default: false).",
      defaultValue: false,
    },
  ],
  tips: [
    "Performs exact string matching - ensure old_string matches exactly",
    "Use replace_all=true to replace all occurrences",
    "For complex edits, consider apply_patch or write_to_file",
  ],
};
