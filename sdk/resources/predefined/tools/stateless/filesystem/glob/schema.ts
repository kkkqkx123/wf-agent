/**
 * The `glob` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * glob tool parameter Schema
 *
 * Recursive matching is controlled by the glob pattern itself, not a separate parameter.
 * Use double-asterisk (e.g., '**\/*.ts') for recursive matching, single asterisk (e.g., '*.ts') for single-directory.
 */
export const globSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    path: {
      type: "string",
      description: "Directory path to search, relative to the workspace",
    },
    pattern: {
      type: "string",
      description:
        "Glob pattern to match file/directory names. Use '**' for recursive matching (e.g., '**/*.ts'), or '*' for top-level only (e.g., '*.ts')",
    },
    includeIgnored: {
      type: "boolean",
      description:
        "Include typically ignored directories (node_modules, .git, target, etc.). Set to true only when you specifically need to search inside these directories. (default: false)",
    },
  },
  required: ["path", "pattern"],
};
