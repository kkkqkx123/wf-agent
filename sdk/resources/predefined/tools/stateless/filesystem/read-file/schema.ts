/**
 * The `read_file` tool parameter `Schema` is defined as follows:
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * read_file tool parameter Schema
 */
export const readFileSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    path: {
      type: "string",
      description: "Absolute or relative path to the file",
    },
    offset: {
      type: "integer",
      description:
        "Starting line number (1-indexed). Use for large files to read from specific line",
      minimum: 1,
    },
    limit: {
      type: "integer",
      description: "Number of lines to read. Use with offset for large files to read in chunks",
      minimum: 1,
    },
  },
  required: ["path"],
};
