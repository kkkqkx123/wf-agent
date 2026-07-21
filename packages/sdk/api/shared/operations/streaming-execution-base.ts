/**
 * StreamingExecutionBase - Shared base class for streaming execution commands
 *
 * Provides a unified infrastructure for event-driven streaming execution.
 * Both Agent and Workflow streaming commands extend this class, sharing:
 * - Event queue management with async generator pattern
 * - Event subscription/unsubscription lifecycle
 * - Promise.race based event-driven yielding
 * - Error handling and cleanup
 *
 * Subclasses must implement:
 * - startExecution(): Start the execution and return the execution ID and event emitter
 * - getEventTypes(): Return the list of event types to subscribe to
 */

import type { BaseEvent, EventType } from "@wf-agent/types";

/**
 * Event emitter interface for subscription
 */
export interface EventEmitterLike {
  on(eventType: EventType, listener: (event: BaseEvent) => void): () => void;
}

/**
 * Execution context returned by startExecution
 */
export interface ExecutionContext {
  /** Execution ID for event filtering */
  executionId: string;
  /** Event emitter instance */
  emitter: EventEmitterLike;
  /** Promise that resolves when execution completes */
  executionPromise: Promise<void>;
}

/**
 * StreamingExecutionBase - Abstract base class for streaming execution
 */
export abstract class StreamingExecutionBase {
  /**
   * Start the execution and return the execution context.
   * Subclasses must implement this to provide the execution-specific logic.
   */
  protected abstract startExecution(): Promise<ExecutionContext>;

  /**
   * Get the list of event types to subscribe to.
   * Subclasses must implement this to provide the relevant event types.
   */
  protected abstract getEventTypes(): EventType[];

  /**
   * Execute the stream and yield events as they occur.
   * Uses a shared event queue and Promise.race pattern for efficient event-driven streaming.
   * @returns AsyncGenerator of events
   */
  async *executeStream(): AsyncGenerator<BaseEvent> {
    const { executionId, emitter, executionPromise } = await this.startExecution();

    const eventQueue: BaseEvent[] = [];
    let resolveEvent: ((value: IteratorResult<BaseEvent>) => void) | null = null;
    let executionComplete = false;

    const eventListener = (event: BaseEvent) => {
      if (event.executionId === executionId) {
        eventQueue.push(event);
        if (resolveEvent) {
          const nextEvent = eventQueue.shift()!;
          resolveEvent({ value: nextEvent, done: false });
          resolveEvent = null;
        }
      }
    };

    const eventTypes = this.getEventTypes();
    const unsubscribers: Array<() => void> = [];

    for (const eventType of eventTypes) {
      const unsubscribe = emitter.on(eventType, eventListener);
      unsubscribers.push(unsubscribe);
    }

    // Track execution completion
    const trackedPromise = executionPromise
      .then(() => {
        executionComplete = true;
      })
      .catch((error) => {
        executionComplete = true;
        throw error;
      });

    try {
      while (!executionComplete || eventQueue.length > 0) {
        if (eventQueue.length > 0) {
          yield eventQueue.shift()!;
        } else if (!executionComplete) {
          await new Promise<void>((resolve) => {
            resolveEvent = (value) => {
              if (value.done === false) {
                resolve();
              }
            };
          });
        }
      }
    } finally {
      for (const unsubscribe of unsubscribers) {
        unsubscribe();
      }
    }

    await trackedPromise;
  }
}