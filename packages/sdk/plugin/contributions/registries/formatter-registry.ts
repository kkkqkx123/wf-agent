/**
 * Formatter Registry - Internal registry for plugin-contributed formatters.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { BaseFormatter } from "../../../services/llm/formatters/base.js";
import { BaseContributionRegistry, type ContributionEntry } from "./base-contribution-registry.js";

interface FormatterEntry extends ContributionEntry {
  formatter: BaseFormatter;
}

export class FormatterRegistry extends BaseContributionRegistry<FormatterEntry> {
  register(pluginId: string, name: string, formatter: BaseFormatter): void {
    this.entries.set(name, { pluginId, formatter });
  }

  getFormatter(name: string): BaseFormatter | undefined {
    return this.entries.get(name)?.formatter;
  }
}