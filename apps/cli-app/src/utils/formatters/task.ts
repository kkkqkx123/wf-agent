/**
 * Task formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, shortId, emptyMsg } from "./utils.js";

export function formatTask(task: any, options?: { verbose?: boolean }): string {
  return formatWith(task, options, () => {
    const id = task.id || "N/A";
    const status = task.status || "unknown";
    const type = task.instanceType || "N/A";
    return `${shortId(id)} - ${status} - ${type}`;
  });
}

export function formatTaskList(tasks: any[], options?: { table?: boolean }): string {
  if (tasks.length === 0) {
    return emptyMsg("tasks");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["Task ID", "Status", "Type", "Execution ID"];
    const rows = tasks.map(t => [
      shortId(t.id),
      t.status || "unknown",
      t.instanceType || "N/A",
      shortId(t.instance?.id),
    ]);
    return formatter.table(headers, rows);
  }

  return tasks.map(t => formatTask(t)).join("\n");
}

export function formatTaskStats(stats: {
  total: number;
  queued: number;
  running: number;
  completed: number;
  failed: number;
  cancelled: number;
  timeout: number;
}): string {
  const formatter = getFormatter();
  const lines: string[] = [];
  lines.push(formatter.subsection("Task Statistics:"));
  lines.push(formatter.keyValue("Total", String(stats.total)));
  lines.push(formatter.keyValue("Queued", String(stats.queued)));
  lines.push(formatter.keyValue("Running", String(stats.running)));
  lines.push(formatter.keyValue("Completed", String(stats.completed)));
  lines.push(formatter.keyValue("Failed", String(stats.failed)));
  lines.push(formatter.keyValue("Cancelled", String(stats.cancelled)));
  lines.push(formatter.keyValue("Timeout", String(stats.timeout)));
  return lines.join("\n");
}