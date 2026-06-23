/**
 * Execution Event Bus
 *
 * Pub/Sub system for execution state changes and events.
 *
 * ## Design
 *
 * - **Single Source of Truth**: State changes are the source; events are derived from them
 * - **Type Safe**: Strongly typed event handlers for each event type
 * - **Decoupled**: Subscribers don't need to know about each other
 * - **Efficient**: Subscribers only get events they're interested in
 *
 * ## Usage
 *
 * ```typescript
 * // Publish an error event
 * eventBus.publish('error_occurred', {
 *   id: 'error-1',
 *   executionId: 'exec-1',
 *   timestamp: Date.now(),
 *   error: { message: "Something went wrong", ... }
 * });
 *
 * // Subscribe to error events
 * eventBus.on('error_occurred', (event) => {
 *   console.log('Error:', event.error.message);
 * });
 *
 * // Subscribe to all events
 * eventBus.on('*', (type, event) => {
 *   console.log('Event:', type, event);
 * });
 * ```
 */

import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { ID, Timestamp } from "@wf-agent/types";
import type { ExecutionErrorRecord, ExecutionInterruptionRecord } from "@wf-agent/types";

const logger = createContextualLogger({ component: "ExecutionEventBus" });

/**
 * Execution State Changed Event
 */
export interface ExecutionStateChangedEvent {
  type: "state_changed";
  executionId: ID;
  timestamp: Timestamp;
  /** Previous status */
  previousStatus?: string;
  /** New status */
  newStatus: string;
  /** State changes summary */
  changes?: Record<string, unknown>;
}

/**
 * Error Occurred Event
 */
export interface ErrorOccurredEvent {
  type: "error_occurred";
  executionId: ID;
  timestamp: Timestamp;
  error: ExecutionErrorRecord;
  /** Which iteration or node was executing */
  context?: {
    iteration?: number;
    nodeId?: string;
  };
}

/**
 * Interruption Occurred Event
 */
export interface InterruptionOccurredEvent {
  type: "interruption_occurred";
  executionId: ID;
  timestamp: Timestamp;
  interruption: ExecutionInterruptionRecord;
}

/**
 * Tool Executed Event
 */
export interface ToolExecutedEvent {
  type: "tool_executed";
  executionId: ID;
  timestamp: Timestamp;
  toolName: string;
  status: "success" | "failed" | "timeout";
  duration: number;
  /** Optional details about the execution */
  details?: Record<string, unknown>;
}

/**
 * Iteration Started Event (Agent Loop)
 */
export interface IterationStartedEvent {
  type: "iteration_started";
  executionId: ID;
  timestamp: Timestamp;
  iteration: number;
}

/**
 * Iteration Completed Event (Agent Loop)
 */
export interface IterationCompletedEvent {
  type: "iteration_completed";
  executionId: ID;
  timestamp: Timestamp;
  iteration: number;
  result?: Record<string, unknown>;
}

/**
 * Union of all execution events
 */
export type ExecutionEvent =
  | ExecutionStateChangedEvent
  | ErrorOccurredEvent
  | InterruptionOccurredEvent
  | ToolExecutedEvent
  | IterationStartedEvent
  | IterationCompletedEvent;

/**
 * Type-safe event handler
 */
export type EventHandler<T extends ExecutionEvent = ExecutionEvent> = (event: T) => void | Promise<void>;

/**
 * Catch-all event handler for any event
 */
export type AnyEventHandler = (type: ExecutionEvent["type"], event: ExecutionEvent) => void | Promise<void>;

/**
 * ExecutionEventBus
 *
 * Manages publication and subscription of execution events.
 */
export class ExecutionEventBus {
  private handlers: Map<ExecutionEvent["type"] | "*", Set<EventHandler | AnyEventHandler>> = new Map();
  private errorHandlers: Set<(error: Error) => void> = new Set();

  /**
   * Publish an event to all subscribed handlers
   */
  async publish<T extends ExecutionEvent>(event: T): Promise<void> {
    try {
      // Get handlers for this specific event type
      const typeHandlers = this.handlers.get(event.type) || new Set();
      const allHandlers = this.handlers.get("*") || new Set();

      logger.debug(`Publishing event: ${event.type}`, {
        executionId: event.executionId,
        timestamp: event.timestamp,
      });

      // Call type-specific handlers
      for (const handler of typeHandlers) {
        try {
          await (handler as EventHandler<T>)(event);
        } catch (error) {
          this.handleError(error as Error);
        }
      }

      // Call catch-all handlers
      for (const handler of allHandlers) {
        try {
          await (handler as AnyEventHandler)(event.type, event);
        } catch (error) {
          this.handleError(error as Error);
        }
      }
    } catch (error) {
      this.handleError(error as Error);
    }
  }

  /**
   * Subscribe to events of a specific type
   */
  on<T extends ExecutionEvent["type"]>(
    type: T,
    handler: T extends "*" ? AnyEventHandler : EventHandler<Extract<ExecutionEvent, { type: T }>>,
  ): () => void {
    if (!this.handlers.has(type)) {
      this.handlers.set(type, new Set());
    }

    this.handlers.get(type)!.add(handler as EventHandler | AnyEventHandler);

    logger.debug(`Handler registered for event type: ${type}`);

    // Return unsubscribe function
    return () => {
      this.handlers.get(type)?.delete(handler as EventHandler | AnyEventHandler);
      logger.debug(`Handler unregistered for event type: ${type}`);
    };
  }

  /**
   * Subscribe to a specific event type (one time only)
   *
   * Note: For now, use the returned unsubscribe function to manually unsubscribe
   * after the first event. This simplifies the type system.
   *
   * Example:
   * ```typescript
   * const unsubscribe = eventBus.on('error_occurred', (event) => {
   *   console.log('Error:', event.error.message);
   *   unsubscribe(); // Unsubscribe after first event
   * });
   * ```
   */
  // Note: once() method removed due to TypeScript conditional type complexity
  // Use the unsubscribe function returned by on() instead

  /**
   * Subscribe to error events
   */
  onError(handler: (error: Error) => void): () => void {
    this.errorHandlers.add(handler);
    return () => {
      this.errorHandlers.delete(handler);
    };
  }

  /**
   * Clear all handlers (useful for testing)
   */
  clear(): void {
    this.handlers.clear();
    this.errorHandlers.clear();
    logger.debug("All event handlers cleared");
  }

  /**
   * Get handler count for a specific event type
   */
  getHandlerCount(type?: ExecutionEvent["type"] | "*"): number {
    if (!type) {
      let total = 0;
      for (const handlers of this.handlers.values()) {
        total += handlers.size;
      }
      return total;
    }
    return this.handlers.get(type)?.size ?? 0;
  }

  /**
   * Internal error handling
   */
  private handleError(error: Error): void {
    logger.error("Error in event handler", {
      errorMessage: error.message,
      errorStack: error.stack,
    });

    // Call error handlers
    for (const handler of this.errorHandlers) {
      try {
        handler(error);
      } catch (e) {
        logger.error("Error in error handler", {
          errorMessage: (e as Error).message,
        });
      }
    }
  }
}

/**
 * Global singleton instance
 * (Can be replaced with dependency injection in the future)
 */
let globalEventBus: ExecutionEventBus | null = null;

/**
 * Get the global event bus instance
 */
export function getExecutionEventBus(): ExecutionEventBus {
  if (!globalEventBus) {
    globalEventBus = new ExecutionEventBus();
  }
  return globalEventBus;
}

/**
 * Set the global event bus instance (mainly for testing)
 */
export function setExecutionEventBus(bus: ExecutionEventBus): void {
  globalEventBus = bus;
}

/**
 * Reset the global event bus (for testing)
 */
export function resetExecutionEventBus(): void {
  if (globalEventBus) {
    globalEventBus.clear();
  }
  globalEventBus = null;
}
