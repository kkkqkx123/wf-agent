/**
 * TODO List Fragment Generator
 *
 * Generates dynamic content for a TODO list
 */

import type { TodoItem, TodoStatus } from "@wf-agent/types";
import { isValidTodoItem, normalizeTodoList } from "@wf-agent/types";
import { wrapSection } from "./utils.js";
import { truncateText } from "@wf-agent/sdk";

/**
 * Format TODO list text
 *
 * Supports type safe TodoItem [] input and is also compatible with unknown data sources
 */
export function formatTodoListText(raw: TodoItem[] | unknown): string {
  const todos = Array.isArray(raw) && raw.every(isValidTodoItem) ? raw : normalizeTodoList(raw);
  if (todos.length === 0) return "";

  const order: Record<TodoStatus, number> = {
    in_progress: 0,
    pending: 1,
    completed: 2,
    cancelled: 3,
  };
  const sorted = [...todos].sort((a, b) => {
    const oa = order[a.status] ?? 9;
    const ob = order[b.status] ?? 9;
    if (oa !== ob) return oa - ob;
    return a.id.localeCompare(b.id);
  });

  const counts: Record<TodoStatus, number> = {
    pending: 0,
    in_progress: 0,
    completed: 0,
    cancelled: 0,
  };
  for (const t of todos) {
    const status = t["status"];
    if (status) {
      counts[status]++;
    }
  }

  const MAX_ITEMS = 50;
  const shown = sorted.slice(0, MAX_ITEMS);

  const lines: string[] = [];
  lines.push(
    `Total: ${todos.length} | pending: ${counts["pending"]} | in_progress: ${counts["in_progress"]} | completed: ${counts["completed"]} | cancelled: ${counts["cancelled"]}`,
  );
  for (const t of shown) {
    const content = truncateText(t.content, 200);
    lines.push(`- [${t.status}] ${content}  \`#${t.id}\``);
  }
  if (sorted.length > shown.length) {
    lines.push(`... and ${sorted.length - shown.length} more items.`);
  }

  return lines.join("\n");
}

/**
 * Generate TODO list content
 *
 * Supports type safe TodoItem [] input and is also compatible with unknown data sources
 */
export function generateTodoListContent(todoList?: TodoItem[] | unknown): string {
  const todoText = formatTodoListText(todoList);
  if (!todoText) return "";
  return wrapSection("TODO LIST", todoText);
}
