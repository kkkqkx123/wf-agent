/**
 * Event formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, shortId, emptyMsg } from "./utils.js";
import type { BaseEvent } from "@wf-agent/types";

export function formatEvent(event: BaseEvent, options?: { verbose?: boolean }): string {
  return formatWith(event, options, () => {
    const type = event.type || "N/A";
    const timestamp = event.timestamp || "N/A";
    const executionId = event.executionId || "N/A";
    return `${type} - ${timestamp} - ${shortId(executionId)}`;
  });
}

export function formatEventList(events: BaseEvent[], options?: { table?: boolean }): string {
  if (events.length === 0) {
    return emptyMsg("events");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["Type", "Time", "Execution ID", "Workflow ID"];
    const rows = events.map(e => [
      e.type || "N/A",
      String(e.timestamp || "N/A"),
      shortId(e.executionId),
      shortId(e.workflowId),
    ]);
    return formatter.table(headers, rows);
  }

  return events.map(e => formatEvent(e)).join("\n");
}