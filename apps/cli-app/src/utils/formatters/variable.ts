/**
 * Variable formatters
 */

import { getFormatter } from "../formatter.js";
import { emptyMsg } from "./utils.js";

/**
 * A variable entry with a name and value.
 */
export interface VariableEntry {
  name: string;
  value: unknown;
}

export function formatVariable(variable: VariableEntry, options?: { verbose?: boolean }): string {
  const formatter = getFormatter();
  if (options?.verbose) {
    return formatter.json({ name: variable.name, value: variable.value });
  }

  const valueStr = typeof variable.value === "object" ? JSON.stringify(variable.value, null, 2) : String(variable.value);
  return formatter.keyValue(variable.name, valueStr);
}

export function formatVariableList(
  variables: Record<string, unknown>,
  options?: { table?: boolean },
): string {
  const formatter = getFormatter();
  const entries = Object.entries(variables);

  if (entries.length === 0) {
    return emptyMsg("variables");
  }

  if (options?.table) {
    const headers = ["Variable name", "Value", "Type"];
    const rows = entries.map(([name, value]) => [
      name,
      typeof value === "object" ? JSON.stringify(value).substring(0, 30) + "..." : String(value),
      typeof value,
    ]);
    return formatter.table(headers, rows);
  }

  return entries.map(([name, value]) => formatVariable({ name, value })).join("\n");
}