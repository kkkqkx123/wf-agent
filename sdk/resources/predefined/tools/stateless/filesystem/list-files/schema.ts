/**
 * The `list_files` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

export const listFilesSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    path: {
      type: "string",
      description: "Directory path to list, relative to the workspace",
    },
    recursive: {
      type: "boolean",
      description:
        "List recursively. true for recursive listing, false for top-level only (default: false)",
    },
    includeIgnored: {
      type: "boolean",
      description:
        "Include typically ignored directories (node_modules, .git, target, etc.). Set to true only when you specifically need to view files inside these directories. (default: false)",
    },
  },
  required: ["path"],
};