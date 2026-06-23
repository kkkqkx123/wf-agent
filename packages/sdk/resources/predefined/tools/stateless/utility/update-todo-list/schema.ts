/**
 * The `update_todo_list` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * update_todo_list tool parameter Schema
 */
export const updateTodoListSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    todos: {
      type: "string",
      description:
        "Multi-line markdown checklist. Each line must be a separate todo item: [ ] for pending, [-] for in progress, [x] for completed.",
    },
  },
  required: ["todos"],
};
