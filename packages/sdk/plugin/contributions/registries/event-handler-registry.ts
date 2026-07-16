/**
 * Event Handler Registry - Internal registry for plugin-contributed event handlers.
 *
 * @internal - Used internally by ContributionManager.
 */

/**
 * @internal - SDK-internal event handler callback signature.
 */
export type EventHandler = (event: Record<string, unknown>) => void | Promise<void>;

interface EventHandlerEntry {
  pluginId: string;
  handler: EventHandler;
  priority?: number;
}

export class EventHandlerRegistry {
  private entries = new Map<string, EventHandlerEntry>();

  register(pluginId: string, eventType: string, handler: EventHandler, priority?: number): void {
    this.entries.set(eventType, { pluginId, handler, priority });
  }

  getAllHandlers(): Map<string, EventHandler> {
    const result = new Map<string, EventHandler>();
    for (const [eventType, entry] of this.entries) {
      result.set(eventType, entry.handler);
    }
    return result;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [eventType, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(eventType);
      }
    }
  }
}