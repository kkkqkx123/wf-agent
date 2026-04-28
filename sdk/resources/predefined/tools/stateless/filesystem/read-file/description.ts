/**
 * Tool Description for `read_file`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const READ_FILE_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "read_file",
  id: "read_file",
  type: "STATELESS",
  category: "filesystem",
  description:
    "Read file contents from the filesystem. Output always includes line numbers in format 'LINE_NUMBER|LINE_CONTENT' (1-indexed). Supports reading partial content by specifying line offset and limit for large files. You can call this tool multiple times in parallel to read different files simultaneously.",
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description: "Absolute or relative path to the file",
    },
    {
      name: "offset",
      type: "integer",
      required: false,
      description:
        "Starting line number (1-indexed). Use for large files to read from specific line",
      defaultValue: 1,
    },
    {
      name: "limit",
      type: "integer",
      required: false,
      description: "Number of lines to read. Use with offset for large files to read in chunks",
      defaultValue: undefined,
    },
  ],
  tips: [
    "Call this tool multiple times in parallel to read different files simultaneously",
    "Use offset and limit for large files to read in chunks",
    "Output includes line numbers for easy reference",
  ],
};
