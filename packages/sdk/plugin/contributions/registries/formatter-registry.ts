/**
 * Formatter Registry - Internal registry for plugin-contributed formatters.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { BaseFormatter } from "../../../services/llm/formatters/base.js";

interface FormatterEntry {
  pluginId: string;
  formatter: BaseFormatter;
}

export class FormatterRegistry {
  private entries = new Map<string, FormatterEntry>();

  register(pluginId: string, name: string, formatter: BaseFormatter): void {
    this.entries.set(name, { pluginId, formatter });
  }

  getFormatter(name: string): BaseFormatter | undefined {
    return this.entries.get(name)?.formatter;
  }

  getOwner(name: string): string | undefined {
    return this.entries.get(name)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [name, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(name);
      }
    }
  }
}