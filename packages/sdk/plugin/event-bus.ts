/**
 * Plugin Event Bus - Event system for plugin lifecycle events.
 *
 * Provides observability into plugin lifecycle transitions:
 * - discovery, loading, activation, deactivation
 * - error events
 * - configuration changes
 */

import { createContextualLogger } from "../utils/contextual-logger.js";

// ============================================================
// Event Types
// ============================================================

export type PluginEventType =
  | 'plugin:discovered'
  | 'plugin:loading'
  | 'plugin:loaded'
  | 'plugin:activating'
  | 'plugin:activated'
  | 'plugin:deactivating'
  | 'plugin:deactivated'
  | 'plugin:error'
  | 'plugin:config-changed';

// ============================================================
// Event Interfaces
// ============================================================

export interface PluginEvent {
  type: PluginEventType;
  pluginId: string;
  timestamp: number;
  data?: Record<string, unknown>;
}

export type PluginEventListener = (event: PluginEvent) => void;

// ============================================================
// Plugin Event Bus
// ============================================================

/**
 * PluginEventBus - Emits events during plugin lifecycle transitions.
 *
 * External components can subscribe to plugin events for observability,
 * monitoring, or integration purposes.
 */
export class PluginEventBus {
  private logger = createContextualLogger({ component: 'PluginEventBus' });
  private listeners = new Map<PluginEventType, Set<PluginEventListener>>();

  /**
   * Subscribe to a plugin event type.
   * Returns an unsubscribe function.
   */
  on(type: PluginEventType, listener: PluginEventListener): () => void {
    if (!this.listeners.has(type)) {
      this.listeners.set(type, new Set());
    }
    this.listeners.get(type)!.add(listener);

    // Return unsubscribe function
    return () => {
      this.listeners.get(type)?.delete(listener);
    };
  }

  /**
   * Emit a plugin event.
   */
  emit(type: PluginEventType, pluginId: string, data?: Record<string, unknown>): void {
    const event: PluginEvent = {
      type,
      pluginId,
      timestamp: Date.now(),
      data,
    };

    const listeners = this.listeners.get(type);
    if (listeners && listeners.size > 0) {
      for (const listener of listeners) {
        try {
          listener(event);
        } catch (error) {
          this.logger.error(`Error in event listener for '${type}'`, undefined, undefined, error instanceof Error ? error : undefined);
        }
      }
    }
  }

  /**
   * Remove all event listeners.
   */
  removeAll(): void {
    this.listeners.clear();
  }

  /**
   * Get the number of listeners for a specific event type.
   */
  listenerCount(type: PluginEventType): number {
    return this.listeners.get(type)?.size || 0;
  }
}