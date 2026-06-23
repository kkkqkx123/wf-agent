/**
 * The logic executed by the update_todo_list tool
 *
 * This tool updates the TODO list.
 * The handler returns the parsed TODO list.
 */

import type { ToolOutput, TodoItem, TodoStatus } from "@wf-agent/types";

/**
 * Parse the markdown checklist format
 *
 * Supported formats:
 * - [ ] pending
 * - [-] in_progress
 * - [x] completed
 */
function parseTodoList(todos: string): TodoItem[] {
  const lines = todos.split("\n");
  const items: TodoItem[] = [];
  let idCounter = 1;

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    // Match checkbox patterns: [ ], [-], [x]
    const match = trimmed.match(/^\[([ x-])\]\s*(.+)$/);
    if (match) {
      const statusChar = match[1] as string;
      const content = match[2] as string;

      let status: TodoStatus;
      switch (statusChar) {
        case "x":
          status = "completed";
          break;
        case "-":
          status = "in_progress";
          break;
        default:
          status = "pending";
      }

      items.push({
        id: `todo-${idCounter++}`,
        content,
        status,
      });
    }
  }

  return items;
}

/**
 * Create the `update_todo_list` tool execution function
 */
export function createUpdateTodoListHandler() {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { todos } = params as { todos: string };

      if (!todos || typeof todos !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'todos' parameter",
        };
      }

      const items = parseTodoList(todos);

      if (items.length === 0) {
        return {
          success: false,
          content: "",
          error:
            "No valid todo items found. Use format: [ ] pending, [x] completed, [-] in progress",
        };
      }

      // Return a special result indicating TODO list update
      // The workflow engine should handle this by updating the TODO list
      const summary = {
        total: items.length,
        pending: items.filter(i => i.status === "pending").length,
        completed: items.filter(i => i.status === "completed").length,
        inProgress: items.filter(i => i.status === "in_progress").length,
      };

      return {
        success: true,
        content: `TODO list updated:\n${items
          .map(i => {
            const checkbox =
              i.status === "completed" ? "[x]" : i.status === "in_progress" ? "[-]" : "[ ]";
            return `${checkbox} ${i.content}`;
          })
          .join(
            "\n",
          )}\n\nSummary: ${summary.pending} pending, ${summary.inProgress} in progress, ${summary.completed} completed`,
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  };
}
