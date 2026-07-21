/**
 * Plugin formatters
 */

import { getFormatter } from "../formatter.js";
import { emptyMsg } from "./utils.js";

/**
 * Typed plugin interface for formatter consumption.
 */
interface FormattablePlugin {
  manifest?: {
    id?: string;
    name?: string;
    version?: string;
    description?: string;
  };
  status?: string;
}

export function formatPlugin(plugin: FormattablePlugin, options?: { verbose?: boolean; json?: boolean }): string {
  const formatter = getFormatter();
  if (options?.json || options?.verbose) {
    return formatter.json(plugin);
  }

  const manifest = plugin.manifest;
  const name = manifest?.name || manifest?.id || "N/A";
  const version = manifest?.version || "N/A";
  const status = plugin.status || "unknown";
  const description = manifest?.description || "-";

  return `${name} (${version}) - ${status} - ${description}`;
}

export function formatPluginList(
  plugins: FormattablePlugin[],
  options?: { table?: boolean; verbose?: boolean },
): string {
  if (plugins.length === 0) {
    return emptyMsg("plugins");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["ID", "Name", "Version", "Status", "Description"];
    const rows = plugins.map(p => {
      const manifest = p.manifest;
      return [
        manifest?.id?.substring(0, 16) || "N/A",
        manifest?.name || "N/A",
        manifest?.version || "N/A",
        p.status || "unknown",
        (manifest?.description || "-").substring(0, 40),
      ];
    });
    return formatter.table(headers, rows);
  }

  return plugins.map(p => formatPlugin(p, { verbose: options?.verbose })).join("\n");
}