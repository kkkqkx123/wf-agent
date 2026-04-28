/**
 * Tool Description for `grep`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const GREP_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "grep",
  id: "grep",
  type: "STATELESS",
  category: "filesystem",
  description: `Request to perform a regex search across files in a specified directory, providing context-rich results. This tool searches for patterns or specific content across multiple files, displaying each match with encapsulating context.

Craft your regex patterns carefully to balance specificity and flexibility. Use this tool to find code patterns, TODO comments, function definitions, or any text-based information across the project. The results include surrounding context, so analyze the surrounding code to better understand the matches.`,
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description:
        "The path of the directory to search in (relative to the current workspace directory). This directory will be recursively searched.",
    },
    {
      name: "regex",
      type: "string",
      required: true,
      description: "The regular expression pattern to search for. Uses Rust regex syntax.",
    },
    {
      name: "file_pattern",
      type: "string",
      required: false,
      description:
        "Glob pattern to filter files (e.g., '*.ts' for TypeScript files). If not provided, it will search all files (*).",
    },
  ],
  tips: [
    "Use regex patterns to find code patterns, TODOs, function definitions",
    "Use file_pattern to limit search to specific file types",
    "Results include surrounding context for better understanding",
  ],
};
