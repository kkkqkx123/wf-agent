/**
 * Trigger & TriggerTemplate formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, shortId, emptyMsg } from "./utils.js";
import type { Trigger, TriggerTemplate, TriggerTemplateSummary } from "@wf-agent/types";

// Type alias for trigger with type and executionId fields
type TriggerWithMetadata = Trigger & {
  type?: string;
  executionId?: string;
};

export function formatTrigger(trigger: TriggerWithMetadata, options?: { verbose?: boolean }): string {
  return formatWith(trigger, options, () => {
    const status = trigger.status || "unknown";
    const type = trigger.type || "N/A";
    return `${trigger.id || "N/A"} - ${status} - ${type} - ${trigger.executionId || "N/A"}`;
  });
}

export function formatTriggerList(triggers: TriggerWithMetadata[], options?: { table?: boolean }): string {
  if (triggers.length === 0) {
    return emptyMsg("triggers");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["Trigger ID", "Type", "Status", "Execution ID"];
    const rows = triggers.map(t => [
      shortId(t.id),
      t.type || "N/A",
      t.status || "unknown",
      shortId(t.executionId),
    ]);
    return formatter.table(headers, rows);
  }

  return triggers.map(t => formatTrigger(t)).join("\n");
}

// ============================================
// Trigger Template Formatters
// ============================================

export function formatTriggerTemplate(template: TriggerTemplate, options?: { verbose?: boolean }): string {
  return formatWith(template, options, () => {
    const name = template.name || "N/A";
    const description = template.description || "-";
    const conditionType = template.condition?.eventType || "N/A";
    const actionType = template.action?.type || "N/A";
    return `${name} - ${description} (${conditionType} -> ${actionType})`;
  });
}

/**
 * Internal: format a single TriggerTemplateSummary (used by the list function).
 */
function formatTriggerTemplateSummary(summary: TriggerTemplateSummary): string {
  const name = summary.name || "N/A";
  const description = summary.description || "-";
  const category = summary.category ? ` [${summary.category}]` : "";
  return `${name}${category} - ${description}`;
}

export function formatTriggerTemplateList(
  templates: TriggerTemplateSummary[],
  options?: { table?: boolean },
): string {
  if (templates.length === 0) {
    return emptyMsg("trigger templates");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["Name", "Description", "Category", "Created At"];
    const rows = templates.map(t => [
      t.name,
      (t.description?.substring(0, 40) || "-"),
      t.category || "-",
      String(t.createdAt).substring(0, 10),
    ]);
    return formatter.table(headers, rows);
  }

  return templates.map(t => formatTriggerTemplateSummary(t)).join("\n");
}