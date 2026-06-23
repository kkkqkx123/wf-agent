/**
 * The `grep` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * grep tool parameter Schema
 */
export const grepSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    path: {
      type: "string",
      description: "Directory to search recursively, relative to the workspace",
    },
    regex: {
      type: "string",
      description: "Rust-compatible regular expression pattern to match",
    },
    file_pattern: {
      type: "string",
      nullable: true,
      description: "Optional glob to limit which files are searched (e.g., *.ts)",
    },
  },
  required: ["path", "regex"],
};
