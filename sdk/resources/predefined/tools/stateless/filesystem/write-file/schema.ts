/**
 * The `write_file` tool parameter `Schema` is defined as follows:
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * Write_file tool parameter Schema
 */
export const writeFileSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    path: {
      type: "string",
      description: "Absolute or relative path to the file",
    },
    content: {
      type: "string",
      description: "Complete content to write (will replace existing content)",
    },
  },
  required: ["path", "content"],
};
