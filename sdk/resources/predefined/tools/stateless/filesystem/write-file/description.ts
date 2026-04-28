/**
 * Tool Description for `write_file`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const WRITE_FILE_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "write_file",
  id: "write_file",
  type: "STATELESS",
  category: "filesystem",
  description:
    "Write content to a file. Will overwrite existing files completely. For existing files, you should read the file first using read_file. Prefer editing existing files over creating new ones unless explicitly needed.",
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
    "Will overwrite existing files completely",
    "Read existing files first before overwriting",
    "Prefer editing over creating new files when possible",
  ],
};
