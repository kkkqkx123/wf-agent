/**
 * Hook Handler Registry - Internal registry for plugin-contributed hook handlers.
 *
 * @internal - Used internally by ContributionManager.
 */

import { BaseContributionRegistry, type ContributionEntry } from "./base-contribution-registry.js";

/**
 * @internal - SDK-internal hook handler callback signature.
 */
export type HookHandler = (context: Record<string, unknown>) => Promise<void>;

interface HookHandlerEntry extends ContributionEntry {
  handler: HookHandler;
}

export class HookHandlerRegistry extends BaseContributionRegistry<HookHandlerEntry> {
  register(pluginId: string, hookType: string, handler: HookHandler): void {
    this.entries.set(hookType, { pluginId, handler });
  }

  getAllHandlers(): Map<string, HookHandler> {
    const result = new Map<string, HookHandler>();
    for (const [hookType, entry] of this.entries) {
      result.set(hookType, entry.handler);
    }
    return result;
  }
}