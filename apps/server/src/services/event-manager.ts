/**
 * Event Manager
 *
 * Central event management for real-time updates.
 * Maintains subscriptions and broadcasts events to listening clients.
 * Uses a broadcaster pattern for pluggable transport layers (WS, SSE, etc.)
 * and a ring buffer for event replay on late-joining clients.
 */

export interface ExecutionEvent {
  type: "status" | "log" | "progress" | "error" | "complete";
  executionId: string;
  timestamp: string;
  data: Record<string, any>;
}

type EventHandler = (event: ExecutionEvent) => void;

/**
 * Interface for pluggable event broadcasters.
 * Implementations (e.g. WSManager) register themselves to receive
 * every emitted event via onEvent().
 */
export interface IEventBroadcaster {
  readonly name: string;
  onEvent(event: ExecutionEvent): void;
}

export interface EventManagerOptions {
  /** Max events kept in the replay buffer (0 = disabled). Default: 100. */
  maxBufferSize: number;
}

const DEFAULT_OPTIONS: EventManagerOptions = {
  maxBufferSize: 100,
};

export class EventManager {
  private subscriptions: Map<string, Set<EventHandler>> = new Map();
  private wildcardSubscriptions: Set<EventHandler> = new Set();
  private broadcasters: Map<string, IEventBroadcaster> = new Map();
  private eventBuffer: ExecutionEvent[] = [];
  private options: EventManagerOptions;

  constructor(options?: Partial<EventManagerOptions>) {
    this.options = { ...DEFAULT_OPTIONS, ...options };
  }

  /**
   * Register a broadcaster to receive all emitted events.
   */
  registerBroadcaster(broadcaster: IEventBroadcaster): void {
    if (this.broadcasters.has(broadcaster.name)) {
      return;
    }
    this.broadcasters.set(broadcaster.name, broadcaster);
  }

  /**
   * Unregister a previously registered broadcaster.
   */
  unregisterBroadcaster(name: string): void {
    this.broadcasters.delete(name);
  }

  /**
   * Subscribe to execution events.
   * Use "*" as executionId to subscribe to all events.
   * Returns an unsubscribe function.
   */
  subscribe(executionId: string, handler: EventHandler): () => void {
    // Replay buffered events for new subscribers
    if (executionId === "*") {
      for (const event of this.eventBuffer) {
        try {
          handler(event);
        } catch {
          // Ignore replay errors
        }
      }
      this.wildcardSubscriptions.add(handler);
      return () => {
        this.wildcardSubscriptions.delete(handler);
      };
    }

    if (!this.subscriptions.has(executionId)) {
      this.subscriptions.set(executionId, new Set());
    }

    // Replay buffered events for this execution
    for (const event of this.eventBuffer) {
      if (event.executionId === executionId) {
        try {
          handler(event);
        } catch {
          // Ignore replay errors
        }
      }
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
   * Emit event to all subscribers and broadcasters.
   */
  emit(event: ExecutionEvent): void {
    // 1. Buffer the event for replay
    this.bufferEvent(event);

    // 2. Notify execution-specific subscribers
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

    // 3. Notify wildcard subscribers
    this.wildcardSubscriptions.forEach((handler) => {
      try {
        handler(event);
      } catch (error) {
        console.error(`Error in wildcard event handler: ${error}`);
      }
    });

    // 4. Notify all registered broadcasters (e.g. WS, SSE)
    for (const broadcaster of this.broadcasters.values()) {
      try {
        broadcaster.onEvent(event);
      } catch (error) {
        console.error(`Error in broadcaster ${broadcaster.name}: ${error}`);
      }
    }
  }

  /**
   * Add event to the ring buffer, trimming oldest entries.
   */
  private bufferEvent(event: ExecutionEvent): void {
    if (this.options.maxBufferSize <= 0) return;
    this.eventBuffer.push(event);
    if (this.eventBuffer.length > this.options.maxBufferSize) {
      this.eventBuffer.shift();
    }
  }

  /**
   * Replay buffered events to a handler (for late-joining clients).
   * Used by broadcasters (SSE, WS) to replay history without subscribing.
   * Pass null as executionId to replay all buffered events.
   */
  replayEvents(executionId: string | null, handler: EventHandler): void {
    for (const event of this.eventBuffer) {
      if (executionId === null || event.executionId === executionId) {
        try {
          handler(event);
        } catch {
          // Ignore replay errors
        }
      }
    }
  }

  /**
   * Get subscriber count for execution
   */
  getSubscriberCount(executionId: string): number {
    return this.subscriptions.get(executionId)?.size || 0;
  }

  /**
   * Get the current replay buffer size
   */
  getBufferSize(): number {
    return this.eventBuffer.length;
  }

  /**
   * Clear all subscriptions (for cleanup)
   */
  clear(): void {
    this.subscriptions.clear();
    this.wildcardSubscriptions.clear();
    this.eventBuffer = [];
  }
}