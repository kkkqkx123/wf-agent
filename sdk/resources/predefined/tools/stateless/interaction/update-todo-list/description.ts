/**
 * Tool Description for `update_todo_list`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const UPDATE_TODO_LIST_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "update_todo_list",
  id: "update_todo_list",
  type: "STATELESS",
  category: "code",
  description: `Replace the entire TODO list. Always provide the full list; the system will overwrite the previous one.

Format: Multi-line markdown checklist. Each line is one item:
[ ] pending task
[-] in progress task
[x] completed task`,
  parameters: [
    {
      name: "todos",
      type: "string",
      required: true,
      description:
        "Multi-line markdown checklist. Each line must be a separate todo item: [ ] for pending, [-] for in progress, [x] for completed.",
    },
  ],
  tips: [
    "Always provide the full list - it will overwrite the previous one",
    "Use [ ] for pending, [-] for in progress, [x] for completed",
    "Each line is a separate todo item",
  ],
};
