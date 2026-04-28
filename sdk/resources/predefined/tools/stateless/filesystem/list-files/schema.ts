/**
 * The `list_files` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * list_files tool parameter Schema
 */
export const listFilesSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    path: {
      type: "string",
      description: "Directory path to inspect, relative to the workspace",
    },
    recursive: {
      type: "boolean",
      description:
        "Set true to list contents recursively; false to show only the top level (default: false)",
    },
  },
  required: ["path"],
};
