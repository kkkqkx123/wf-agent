/**
 * Tool Description for `edit`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const EDIT_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "edit",
  type: "STATELESS",
  category: "filesystem",
  description: `Edit a file by searching for and replacing a specific string.

Safety features:
- By default, requires the search string to be unique in the file (require_unique=true)
- Fuzzy matching (Unicode normalization) is only enabled when require_unique is true or not specified
- When require_unique=false, exact matching is enforced for batch replacements

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
        "If true (default), the old_string must appear exactly once in the file. This prevents accidental replacements in multiple locations. Fuzzy matching (Unicode normalization) is enabled in this mode. If false, allows batch replacements but enforces exact matching and disables fuzzy matching (default: true).",
      defaultValue: true,
    },
  ],
  tips: [
    "By default, requires unique match (require_unique=true) for safety",
    "Supports Unicode normalization (fancy quotes, dashes, etc.) when require_unique=true",
    "Set require_unique=false for batch replacements (exact matching only, no fuzzy match)",
    "Use replace_all=true with require_unique=false to replace all occurrences",
    "For complex code edits, use apply_diff or apply_patch instead",
  ],
};
