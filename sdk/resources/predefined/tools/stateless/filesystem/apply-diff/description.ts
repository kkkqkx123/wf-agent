/**
 * Tool Description for `apply_diff`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const APPLY_DIFF_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "apply_diff",
  id: "apply_diff",
  type: "STATELESS",
  category: "filesystem",
  description: `Apply a diff to a file. This tool applies changes to a file using a unified diff format, which shows exactly what lines to remove and add with context to uniquely identify the changes.

The diff should include sufficient context (typically 3 lines) to uniquely identify the location of the change. Use this tool for precise edits when you know the exact content to change.`,
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description: "The path of the file to edit (relative to the current workspace directory)",
    },
    {
      name: "diff",
      type: "string",
      required: true,
      description:
        "The diff content to apply. Should follow unified diff format with context lines to uniquely identify the changes.",
    },
  ],
  tips: [
    "Include sufficient context (typically 3 lines) to uniquely identify the change location",
    "Use for precise edits when you know the exact content to change",
    "Follows unified diff format with --- and +++ headers",
  ],
};
