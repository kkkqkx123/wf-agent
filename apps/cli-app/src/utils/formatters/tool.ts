/**
 * Tool formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, shortId, emptyMsg } from "./utils.js";
import type { Tool } from "@wf-agent/types";

// Type alias for tool with name field
type ToolWithName = Tool & {
  name?: string;
};

export function formatTool(tool: ToolWithName, options?: { verbose?: boolean }): string {
  return formatWith(tool, options, () => {
    const type = tool.type || "N/A";
    return `${tool.name || "unnamed"} (${tool.id || "N/A"}) - ${type}`;
  });
}

export function formatToolList(tools: ToolWithName[], options?: { table?: boolean }): string {
  if (tools.length === 0) {
    return emptyMsg("tools");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["ID", "Name", "Type", "Description"];
    const rows = tools.map(t => [
      shortId(t.id),
      t.name || "unnamed",
      t.type || "N/A",
      t.description || "-",
    ]);
    return formatter.table(headers, rows);
  }

  return tools.map(t => formatTool(t)).join("\n");
}