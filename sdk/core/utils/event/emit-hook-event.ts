/**
 * EmitHookEvent - Shared hook event emission utility
 *
 * Provides a unified try/catch error handling wrapper for hook event emission,
 * eliminating the duplicated pattern between workflow and agent hook handlers.
 *
 * Usage:
 * ```typescript
 * import { emitHookEventSafe } from "../../core/utils/event/emit-hook-event.js";
 *
 * await emitHookEventSafe(
 *   event,
 *   emitEvent,
 *   `Failed to emit custom event "${eventName}" for node "${node.id}"`,
 *   { operation: "emit", eventType: "NODE_CUSTOM_EVENT", nodeId: node.id },
 *   { eventName, nodeId: node.id },
 * );
 * ```
 */

import { EventSystemError } from "@wf-agent/types";
import { getErrorOrNew } from "@wf-agent/common-utils";

/**
 * Safely emit a hook event with consistent error handling
 *
 * @param event - The event object to emit
 * @param emitEvent - The event emitter function
 * @param errorMessage - Human-readable error message prefix
 * @param errorContext - Context for the EventSystemError
 * @param additionalContext - Additional context to attach to the error
 * @throws EventSystemError if emission fails
 */
export async function emitHookEventSafe<T>(
  event: T,
  emitEvent: (event: T) => Promise<void>,
  errorMessage: string,
  errorContext: {
    operation: "emit" | "subscribe" | "unsubscribe" | "handle";
    eventType: string;
    nodeId?: string;
  },
  additionalContext?: Record<string, unknown>,
): Promise<void> {
  try {
    await emitEvent(event);
  } catch (error) {
    throw new EventSystemError(
      errorMessage,
      errorContext.operation,
      errorContext.eventType,
      errorContext.nodeId,
      undefined,
      {
        ...additionalContext,
        originalError: getErrorOrNew(error),
      },
    );
  }
}
