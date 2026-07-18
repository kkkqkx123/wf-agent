/**
 * LLM Provider Registry - Internal registry for plugin-contributed LLM providers.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { BaseFormatter } from "../../../services/llm/formatters/base.js";
import { BaseContributionRegistry, type ContributionEntry } from "./base-contribution-registry.js";

interface LLMProviderEntry extends ContributionEntry {
  formatter: BaseFormatter;
}

export class LLMProviderRegistry extends BaseContributionRegistry<LLMProviderEntry> {
  register(pluginId: string, provider: string, formatter: BaseFormatter): void {
    this.entries.set(provider, { pluginId, formatter });
  }

  getFormatter(provider: string): BaseFormatter | undefined {
    return this.entries.get(provider)?.formatter;
  }

  getAllProviders(): string[] {
    return Array.from(this.entries.keys());
  }
}