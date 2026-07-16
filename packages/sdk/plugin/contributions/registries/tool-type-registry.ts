/**
 * Tool Type Registry - Internal registry for plugin-contributed tool types.
 *
 * @internal - Used internally by ContributionManager.
 */

import type { IToolExecutor } from "../../../services/tools/core/interfaces.js";

/**
 * @internal - Constructor type for IToolExecutor implementations.
 */
export type IToolExecutorConstructor = new (...args: unknown[]) => IToolExecutor;

interface ToolTypeEntry {
  pluginId: string;
  executor: IToolExecutorConstructor;
}

export class ToolTypeRegistry {
  private entries = new Map<string, ToolTypeEntry>();

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

  getOwner(type: string): string | undefined {
    return this.entries.get(type)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [type, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(type);
      }
    }
  }
}