/**
 * EmitEvent - An event-triggering utility function
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
import type { Event } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";

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
