/**
 * Hook template formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, emptyMsg } from "./utils.js";
import type { HookTemplate } from "@wf-agent/types";

interface HookTemplateSummary {
  name: string;
  hookType: string;
  description?: string;
  category?: string;
  tags?: string[];
  createdAt: number;
  updatedAt: number;
}

export function formatHookTemplate(template: HookTemplate, options?: { verbose?: boolean }): string {
  return formatWith(template, options, () => {
    const name = template.name || "N/A";
    const hookType = template.hook?.hookType || "N/A";
    const eventName = template.hook?.eventName || "N/A";
    return `${name} - ${hookType} (${eventName})`;
  });
}

export function formatHookTemplateList(
  templates: HookTemplateSummary[],
  options?: { table?: boolean },
): string {
  if (templates.length === 0) {
    return emptyMsg("hook templates");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["Name", "Hook Type", "Description", "Category"];
    const rows = templates.map(t => [
      t.name,
      t.hookType || "N/A",
      (t.description?.substring(0, 40) || "-"),
      t.category || "-",
    ]);
    return formatter.table(headers, rows);
  }

  return templates
    .map(t => {
      const desc = t.description ? ` - ${t.description.substring(0, 50)}` : "";
      const cat = t.category ? ` [${t.category}]` : "";
      return `${t.name}${cat}${desc}`;
    })
    .join("\n");
}