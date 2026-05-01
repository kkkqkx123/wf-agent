/**
 * EventEmitter - An event-triggering utility function
 * Provides a unified method for triggering events, including error handling
 *
 * Design principles:
 * - Pure functions: All methods are pure functions with no side effects.
 * - Error handling: Offers a safe mechanism for error handling.
 * - Simplified invocation: Simplifies the way to trigger events.
 * 
 * IMPORTANT: Events are for state changes and coordination, NOT for logging.
 * Use logger for informational output. If event emission is optional,
 * check eventManager availability before calling emit().
 */

import type { EventRegistry } from "../../registry/event-registry.js";
import type { Event, EventType } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import { getErrorOrNew } from "@wf-agent/common-utils";



/**
 * Trigger an event (which may throw an exception if it fails)
 *
 * @param eventManager  Event manager
 * @param event  Event object
 * @throws Error  If the event manager does not exist or the triggering fails
 */
export async function emit(eventManager: EventRegistry | undefined, event: Event): Promise<void> {
  if (!eventManager) {
    throw new ExecutionError("EventRegistry is not available", undefined, undefined, {
      service: "EventRegistry",
      operation: "event_emit",
    });
  }

  await eventManager.emit(event);
}

/**
 * Batch Triggering of Events
 * If the triggering of an event fails, other events will continue to be triggered.
 * Note: Each event emission failure will throw an error but won't stop other events.
 *
 * @param eventManager: The event manager
 * @param events: The array of events
 */
export async function emitBatch(
  eventManager: EventRegistry | undefined,
  events: Event[],
): Promise<void> {
  if (!eventManager) {
    throw new ExecutionError("EventRegistry is not available", undefined, undefined, {
      service: "EventRegistry",
      operation: "event_batch_emit",
    });
  }

  const errors: Error[] = [];
  for (const event of events) {
    try {
      await eventManager.emit(event);
    } catch (error) {
      errors.push(getErrorOrNew(error));
    }
  }

  if (errors.length > 0) {
    throw new ExecutionError(
      `${errors.length} out of ${events.length} events failed to emit`,
      undefined,
      undefined,
      { operation: "event_batch_emit", failures: errors.length },
      errors[0],
    );
  }
}

/**
 * Batch security trigger events (parallel)
 * All events are triggered in parallel, and a failure does not affect the others.
 * Note: Failures are collected and reported after all events are attempted.
 *
 * @param eventManager: Event manager
 * @param events: Array of events
 */
export async function emitBatchParallel(
  eventManager: EventRegistry | undefined,
  events: Event[],
): Promise<void> {
  if (!eventManager) {
    throw new ExecutionError("EventRegistry is not available", undefined, undefined, {
      service: "EventRegistry",
      operation: "event_batch_parallel_emit",
    });
  }

  const results = await Promise.allSettled(events.map(event => eventManager.emit(event)));
  
  const errors = results
    .filter((result): result is PromiseRejectedResult => result.status === "rejected")
    .map(result => getErrorOrNew(result.reason));

  if (errors.length > 0) {
    throw new ExecutionError(
      `${errors.length} out of ${events.length} events failed to emit`,
      undefined,
      undefined,
      { operation: "event_batch_parallel_emit", failures: errors.length },
      errors[0],
    );
  }
}

/**
 * Condition-Triggered Event
 * The event is triggered only when the condition is met.
 *
 * @param eventManager  Event Manager
 * @param event  Event Object
 * @param condition  Condition Function
 */
export async function emitIf(
  eventManager: EventRegistry | undefined,
  event: Event,
  condition: () => boolean,
): Promise<void> {
  if (!eventManager) {
    throw new ExecutionError("EventRegistry is not available", undefined, undefined, {
      service: "EventRegistry",
      operation: "event_conditional_emit",
    });
  }
  
  if (!condition()) {
    return;
  }

  await eventManager.emit(event);
}

/**
 * Delayed Event Trigger
 * Triggers an event after a specified delay
 *
 * @param eventManager  Event Manager
 * @param event  Event Object
 * @param delay  Delay Time (in milliseconds)
 */
export async function emitDelayed(
  eventManager: EventRegistry | undefined,
  event: Event,
  delay: number,
): Promise<void> {
  if (!eventManager) {
    throw new ExecutionError("EventRegistry is not available", undefined, undefined, {
      service: "EventRegistry",
      operation: "event_delayed_emit",
    });
  }

  await new Promise(resolve => setTimeout(resolve, delay));
  await eventManager.emit(event);
}

/**
 * Retry Trigger Event
 * If the trigger fails, it will attempt again for the specified number of times.
 *
 * @param eventManager  Event Manager
 * @param event  Event Object
 * @param maxRetries  Maximum number of retries
 * @param retryDelay  Retry delay (in milliseconds)
 */
export async function emitWithRetry(
  eventManager: EventRegistry | undefined,
  event: Event,
  maxRetries: number = 3,
  retryDelay: number = 1000,
): Promise<void> {
  if (!eventManager) {
    return;
  }

  let lastError: Error | undefined;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      await eventManager.emit(event);
      return;
    } catch (error) {
      lastError = error as Error;

      if (attempt < maxRetries) {
        await new Promise(resolve => setTimeout(resolve, retryDelay));
      }
    }
  }

  // All retries failed, and an error was thrown.
  throw new ExecutionError(
    `All ${maxRetries + 1} event emit attempts failed`,
    undefined,
    undefined,
    {
      eventType: event.type,
      operation: "event_emit_retry",
      maxRetries,
    },
    lastError,
  );
}

/**
 * Trigger an event and wait for the callback
 * After triggering an event, wait for the specified callback event to occur.
 *
 * @param eventManager  Event manager
 * @param event  Event object
 * @param callbackEventType  Callback event type
 * @param timeout  Timeout period (in milliseconds)
 */
export async function emitAndWaitForCallback(
  eventManager: EventRegistry | undefined,
  event: Event,
  callbackEventType: EventType,
  timeout: number = 30000,
): Promise<void> {
  if (!eventManager) {
    throw new ExecutionError("EventRegistry is not available", undefined, undefined, {
      service: "EventRegistry",
      operation: "emit_and_wait_for_callback",
    });
  }

  // Trigger event
  await eventManager.emit(event);

  // Waiting for the callback event.
  await eventManager.waitFor(callbackEventType, timeout);
}
