/**
 * Tool Description for `list_files`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const LIST_FILES_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "list_files",
  id: "list_files",
  type: "STATELESS",
  category: "filesystem",
  description: `List files and directories within the specified directory. If recursive is true, it will list all files and directories recursively.`,
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description:
        "The path of the directory to list contents for (relative to the current workspace directory)",
    },
    {
      name: "recursive",
      type: "boolean",
      required: false,
      description:
        "Whether to list files recursively. Use true for recursive listing, false for top-level only (default: false).",
      defaultValue: false,
    },
  ],
  tips: [
    "Use recursive=true to list all files in subdirectories",
    "Use recursive=false (default) for top-level only",
    "Results show files and directories with their types",
  ],
};
