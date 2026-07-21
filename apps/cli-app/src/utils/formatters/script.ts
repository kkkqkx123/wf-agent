/**
 * Script formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, shortId, emptyMsg } from "./utils.js";
import type { Script } from "@wf-agent/types";

// Type alias for script with language field
type ScriptWithLanguage = Script & {
  language?: string;
};

export function formatScript(script: ScriptWithLanguage, options?: { verbose?: boolean }): string {
  return formatWith(script, options, () => {
    const language = script.language || "N/A";
    return `${script.name || "unnamed"} (${script.id || "N/A"}) - ${language}`;
  });
}

export function formatScriptList(scripts: ScriptWithLanguage[], options?: { table?: boolean }): string {
  if (scripts.length === 0) {
    return emptyMsg("scripts");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["ID", "Name", "Language", "Description"];
    const rows = scripts.map(s => [
      shortId(s.id),
      s.name || "unnamed",
      s.language || "N/A",
      s.description || "-",
    ]);
    return formatter.table(headers, rows);
  }

  return scripts.map(s => formatScript(s)).join("\n");
}