/**
 * Tool Description for `edit`
 */

import type { ToolDescriptionData } from "@wf-agent/types";

export const EDIT_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "edit",
  type: "STATELESS",
  category: "filesystem",
  description: `Edit a file by searching for and replacing a specific string.

Modes:
- "safe" (default): Requires the search string to be unique in the file. Enables fuzzy matching (Unicode normalization) for handling special characters. Replaces the first match only.
- "batch": No uniqueness check. Uses exact matching (no fuzzy match). Replaces all occurrences of the search string.

When multiple matches exist, choose one approach:
1. Use mode="batch" to replace all occurrences at once
2. Provide a larger old_string with more surrounding context to make it unique, then use mode="safe"

For complex code edits, consider using apply_diff or apply_patch instead.`,
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
      name: "mode",
      type: "string",
      required: false,
      description:
        "Operation mode. 'safe' (default): requires unique match with fuzzy matching enabled, replaces first occurrence. 'batch': no uniqueness check, exact matching only, replaces all occurrences. Allowed values: 'safe', 'batch'.",
      defaultValue: "safe",
    },
  ],
  tips: [
    "Use mode='safe' for single replacements with automatic Unicode normalization",
    "Use mode='batch' to replace all occurrences when the string appears multiple times",
    "If old_string matches multiple locations in safe mode, expand it with surrounding context to make it unique",
    "Verify changes by reading the file after editing",
    "For multi-line or complex edits, prefer apply_diff or apply_patch",
  ],
};
