/**
 * Tool Description for `write_file`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const WRITE_FILE_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "write_file",
  type: "STATELESS",
  category: "filesystem",
  description:
    "Write content to a file. Creates the file if it doesn't exist, or overwrites it completely if it does.",
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description: "Absolute or relative path to the file",
    },
    {
      name: "content",
      type: "string",
      required: true,
      description: "Content to write to the file",
    },
  ],
  tips: [
    "Read existing files first to avoid accidental data loss",
    "Prefer apply_diff or apply_patch for editing existing files",
    "Use for creating new files or complete file replacement",
  ],
};
