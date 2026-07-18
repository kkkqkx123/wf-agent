/**
 * Tool Type Registry - Internal registry for plugin-contributed tool types.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { IToolExecutor } from "../../../services/tools/core/interfaces.js";
import { BaseContributionRegistry, type ContributionEntry } from "./base-contribution-registry.js";

/**
 * @internal - Constructor type for IToolExecutor implementations.
 */
export type IToolExecutorConstructor = new (...args: unknown[]) => IToolExecutor;

interface ToolTypeEntry extends ContributionEntry {
  executor: IToolExecutorConstructor;
}

export class ToolTypeRegistry extends BaseContributionRegistry<ToolTypeEntry> {
  register(pluginId: string, type: string, executor: IToolExecutorConstructor): void {
    this.entries.set(type, { pluginId, executor });
  }

  getExecutor(type: string): IToolExecutorConstructor | undefined {
    return this.entries.get(type)?.executor;
  }

  getAllExecutors(): Map<string, ToolTypeEntry> {
    return new Map(this.entries);
  }

  getAllToolTypes(): string[] {
    return Array.from(this.entries.keys());
  }
}