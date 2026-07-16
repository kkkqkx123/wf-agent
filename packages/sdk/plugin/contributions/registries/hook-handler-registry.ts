/**
 * Hook Handler Registry - Internal registry for plugin-contributed hook handlers.
 *
 * @internal - Used internally by ContributionManager.
 */

/**
 * @internal - SDK-internal hook handler callback signature.
 */
export type HookHandler = (context: Record<string, unknown>) => Promise<void>;

interface HookHandlerEntry {
  pluginId: string;
  handler: HookHandler;
}

export class HookHandlerRegistry {
  private entries = new Map<string, HookHandlerEntry>();

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

  unregisterByPluginId(pluginId: string): void {
    for (const [hookType, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(hookType);
      }
    }
  }
}