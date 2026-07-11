/**
 * Event Manager
 *
 * Central event management for real-time updates.
 * Maintains subscriptions and broadcasts events to listening clients.
 */

export interface ExecutionEvent {
  type: "status" | "log" | "progress" | "error" | "complete";
  executionId: string;
  timestamp: string;
  data: Record<string, any>;
}

type EventHandler = (event: ExecutionEvent) => void;

export class EventManager {
  private subscriptions: Map<string, Set<EventHandler>> = new Map();
  private static instance: EventManager;

  /**
   * Get singleton instance
   */
  static getInstance(): EventManager {
    if (!EventManager.instance) {
      EventManager.instance = new EventManager();
    }
    return EventManager.instance;
  }

  /**
   * Subscribe to execution events
   */
  subscribe(executionId: string, handler: EventHandler): () => void {
    if (!this.subscriptions.has(executionId)) {
      this.subscriptions.set(executionId, new Set());
    }

    this.subscriptions.get(executionId)!.add(handler);

    // Return unsubscribe function
    return () => {
      this.subscriptions.get(executionId)?.delete(handler);
      if (this.subscriptions.get(executionId)?.size === 0) {
        this.subscriptions.delete(executionId);
      }
    };
  }

  /**
   * Emit event to all subscribers
   */
  emit(event: ExecutionEvent): void {
    const handlers = this.subscriptions.get(event.executionId);
    if (handlers) {
      handlers.forEach((handler) => {
        try {
          handler(event);
        } catch (error) {
          console.error(`Error in event handler: ${error}`);
        }
      });
    }
  }

  /**
   * Get subscriber count for execution
   */
  getSubscriberCount(executionId: string): number {
    return this.subscriptions.get(executionId)?.size || 0;
  }

  /**
   * Clear all subscriptions (for cleanup)
   */
  clear(): void {
    this.subscriptions.clear();
  }
}
