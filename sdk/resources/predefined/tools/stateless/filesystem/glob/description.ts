/**
 * Tool Description for `glob`
 */

import type { ToolDescriptionData } from "@wf-agent/types";

export const GLOB_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "glob",
  type: "STATELESS",
  category: "filesystem",
  description: `Find files and directories by name using glob pattern matching. Supports standard glob patterns including *, **, ?, and {...}. Results are sorted with directories first, then files, alphabetically within each group.`,
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description:
        "The directory to search in (relative to the current workspace directory)",
    },
    {
      name: "pattern",
      type: "string",
      required: true,
      description:
        "Glob pattern to match file/directory names against (e.g., '*.ts', '**/*.test.ts', 'src/**/*.js', '*.{json,yaml}')",
    },
    {
      name: "recursive",
      type: "boolean",
      required: false,
      description:
        "Whether to search recursively. Use true for recursive search, false for top-level only (default: true).",
      defaultValue: true,
    },
  ],
  tips: [
    "Use ** to match directories recursively (e.g., 'src/**/*.ts')",
    "Use * to match files within a single directory (e.g., '*.json')",
    "Use ? to match a single character (e.g., 'file-?.txt')",
    "Use {a,b} for alternatives (e.g., '*.{ts,js}')",
    "Dotfiles are included in matching (e.g., '.*' matches .gitignore)",
  ],
};
