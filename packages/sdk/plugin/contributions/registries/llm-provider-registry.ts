/**
 * LLM Provider Registry - Internal registry for plugin-contributed LLM providers.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { BaseFormatter } from "../../../services/llm/formatters/base.js";

interface LLMProviderEntry {
  pluginId: string;
  formatter: BaseFormatter;
}

export class LLMProviderRegistry {
  private entries = new Map<string, LLMProviderEntry>();

  register(pluginId: string, provider: string, formatter: BaseFormatter): void {
    this.entries.set(provider, { pluginId, formatter });
  }

  getFormatter(provider: string): BaseFormatter | undefined {
    return this.entries.get(provider)?.formatter;
  }

  getAllProviders(): string[] {
    return Array.from(this.entries.keys());
  }

  getOwner(provider: string): string | undefined {
    return this.entries.get(provider)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [provider, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(provider);
      }
    }
  }
}