/**
 * The `glob` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * glob tool parameter Schema
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
        "Glob pattern to match file/directory names (e.g., '*.ts', '**/*.test.ts', 'src/**/*.js')",
    },
    recursive: {
      type: "boolean",
      description:
        "Search recursively. true for recursive, false for top-level only (default: true)",
    },
  },
  required: ["path", "pattern"],
};
