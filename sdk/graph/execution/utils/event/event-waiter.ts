/**
 * EventWaiter - Event Waiter Function (Thread-specific part)
 * Currently used only for threads, as only threads require separate lifecycle management.
 *
 * Responsibilities:
 * - Encapsulate thread-related event waiting logic
 * - Provide timeout control
 * - Simplify the way to wait for events
 * - Support event waiting in multiple threads and nodes
 *
 * Design Principles:
 * - Pure functions: All methods are pure functions
 * - Dependency injection through the EventRegistry
 * - Provide a concise interface for waiting
 * - Event-driven approach to avoid polling
 */

import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import { EventType } from "@wf-agent/types";
import {
  WAIT_FOREVER,
  waitForCondition,
  waitForAllConditions,
  waitForAnyCondition,
} from "../../../../core/utils/event/condition-waiter.js";

// Reexport the generic conditional waiter
export { WAIT_FOREVER, waitForCondition, waitForAllConditions, waitForAnyCondition };

/**
 * Wait for the thread pause event
 *
 * @param eventManager Event manager
 * @param threadId Thread ID
 * @param timeout Timeout period in milliseconds; default is 5000ms. Use WAIT_FOREVER or -1 to indicate waiting indefinitely
 * @returns Promise: Resolved when the timeout occurs or the event is triggered
 *
 * @example
 * // Use default timeout (5000ms)
 * await waitForThreadPaused(eventManager, threadId);
 *
 * @example
 * // Custom timeout (10 seconds)
 * await waitForThreadPaused(eventManager, threadId, 10000);
 *
 * @example
 * // Wait indefinitely
 * await waitForThreadPaused(eventManager, threadId, WAIT_FOREVER);
 */
export async function waitForThreadPaused(
  eventManager: EventRegistry,
  threadId: string,
  timeout: number = 5000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;
  await eventManager.waitFor("THREAD_PAUSED", actualTimeout, event => event.threadId === threadId);
}

/**
 * Wait for the thread cancellation event
 *
 * @param eventManager  Event manager
 * @param threadId  Thread ID
 * @param timeout  Timeout period in milliseconds; the default is 5000ms. Use WAIT_foreVER or -1 to indicate waiting indefinitely
 * @returns Promise: Resolved when the timeout occurs or the event is triggered
 */
export async function waitForThreadCancelled(
  eventManager: EventRegistry,
  threadId: string,
  timeout: number = 5000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;
  await eventManager.waitFor(
    "THREAD_CANCELLED",
    actualTimeout,
    event => event.threadId === threadId,
  );
}

/**
 * Wait for the Thread to complete the event
 *
 * @param eventManager  Event manager
 * @param threadId  Thread ID
 * @param timeout  Timeout period in milliseconds; the default is 30000ms. Use WAIT_foreVER or -1 to indicate waiting indefinitely
 * @returns Promise: Resolves when the timeout occurs or the event is triggered
 */
export async function waitForThreadCompleted(
  eventManager: EventRegistry,
  threadId: string,
  timeout: number = 30000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;
  await eventManager.waitFor(
    "THREAD_COMPLETED",
    actualTimeout,
    event => event.threadId === threadId,
  );
}

/**
 * Waiting for the thread failure event
 *
 * @param eventManager  Event manager
 * @param threadId  Thread ID
 * @param timeout  Timeout period in milliseconds; the default is 30000ms. Use WAIT_FOREVER or -1 to indicate waiting indefinitely
 * @returns Promise: Resolved when the timeout occurs or the event is triggered
 */
export async function waitForThreadFailed(
  eventManager: EventRegistry,
  threadId: string,
  timeout: number = 30000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;
  await eventManager.waitFor("THREAD_FAILED", actualTimeout, event => event.threadId === threadId);
}

/**
 * Waiting for the thread recovery event
 *
 * @param eventManager: Event manager
 * @param threadId: Thread ID
 * @param timeout: Timeout period in milliseconds; the default is 5000ms. Use WAIT_foreVER or -1 to indicate waiting indefinitely
 * @returns: Promise, resolved when the timeout is reached or the event is triggered
 */
export async function waitForThreadResumed(
  eventManager: EventRegistry,
  threadId: string,
  timeout: number = 5000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;
  await eventManager.waitFor("THREAD_RESUMED", actualTimeout, event => event.threadId === threadId);
}

/**
 * Wait for any Thread lifecycle event
 *
 * @param eventManager  Event manager
 * @param threadId  Thread ID
 * @param timeout  Timeout period in milliseconds; the default is 5000ms. Use WAIT_foreVER or -1 to wait indefinitely
 * @returns Promise: Resolved when the timeout occurs or any lifecycle event is triggered
 */
export async function waitForAnyLifecycleEvent(
  eventManager: EventRegistry,
  threadId: string,
  timeout: number = 5000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;

  // Use Promise.race to wait for any lifecycle event
  const events: EventType[] = [
    "THREAD_PAUSED",
    "THREAD_CANCELLED",
    "THREAD_COMPLETED",
    "THREAD_FAILED",
    "THREAD_RESUMED",
  ];

  // Create multiple waiting Promises, each using the threadId filter.
  const promises = events.map(eventType =>
    eventManager.waitFor(eventType, actualTimeout, event => event.threadId === threadId),
  );

  // Wait for either of the events to trigger.
  await Promise.race(promises);
}

/**
 * Wait for multiple threads to complete
 *
 * @param eventManager  Event manager
 * @param threadIds  Array of thread IDs
 * @param timeout  Timeout period in milliseconds; the default is 30000ms. Use WAIT_foreVER or -1 to indicate waiting indefinitely
 * @returns Promise: Resolved when all threads have completed or the timeout has expired
 */
export async function waitForMultipleThreadsCompleted(
  eventManager: EventRegistry,
  threadIds: string[],
  timeout: number = 30000,
): Promise<void> {
  const promises = threadIds.map(threadId =>
    waitForThreadCompleted(eventManager, threadId, timeout),
  );

  await Promise.all(promises);
}

/**
 * Wait for any thread to complete
 *
 * @param eventManager  Event manager
 * @param threadIds  Array of thread IDs
 * @param timeout  Timeout period in milliseconds; the default is 30000ms. Use WAIT_foreVER or -1 to indicate waiting indefinitely
 * @returns Promise: Resolves when any thread completes or the timeout occurs, returning the ID of the completed thread
 */
export async function waitForAnyThreadCompleted(
  eventManager: EventRegistry,
  threadIds: string[],
  timeout: number = 30000,
): Promise<string> {
  const promises = threadIds.map(threadId =>
    waitForThreadCompleted(eventManager, threadId, timeout).then(() => threadId),
  );

  return await Promise.race(promises);
}

/**
 * Wait for either a thread to complete or fail
 *
 * @param eventManager  Event manager
 * @param threadIds  Array of thread IDs
 * @param timeout  Timeout period in milliseconds; the default is 30000ms. Use WAIT_foreVER or -1 to indicate waiting indefinitely
 * @returns Promise: Resolves when either a thread completes or fails, returning the thread ID and its status
 */
export async function waitForAnyThreadCompletion(
  eventManager: EventRegistry,
  threadIds: string[],
  timeout: number = 30000,
): Promise<{ threadId: string; status: "COMPLETED" | "FAILED" }> {
  const completedPromises = threadIds.map(threadId =>
    waitForThreadCompleted(eventManager, threadId, timeout).then(() => ({
      threadId,
      status: "COMPLETED" as const,
    })),
  );

  const failedPromises = threadIds.map(threadId =>
    waitForThreadFailed(eventManager, threadId, timeout).then(() => ({
      threadId,
      status: "FAILED" as const,
    })),
  );

  return await Promise.race([...completedPromises, ...failedPromises]);
}

/**
 * Wait for the node to complete
 *
 * @param eventManager  Event manager
 * @param threadId  Thread ID
 * @param nodeId  Node ID
 * @param timeout  Timeout period in milliseconds; the default is 30000ms. Use WAIT_FOREVER or -1 to indicate waiting indefinitely
 * @returns Promise: Resolved when the timeout occurs or the event is triggered
 */
export async function waitForNodeCompleted(
  eventManager: EventRegistry,
  threadId: string,
  nodeId: string,
  timeout: number = 30000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;
  await eventManager.waitFor(
    "NODE_COMPLETED",
    actualTimeout,
    (event: { threadId?: string; nodeId?: string }) =>
      event.threadId === threadId && event.nodeId === nodeId,
  );
}

/**
 * Node waiting failed
 *
 * @param eventManager  Event manager
 * @param threadId  Thread ID
 * @param nodeId  Node ID
 * @param timeout  Timeout period in milliseconds; the default is 30000ms. Use WAIT_foreVER or -1 to indicate waiting indefinitely
 * @returns Promise, resolved when the timeout is reached or the event is triggered
 */
export async function waitForNodeFailed(
  eventManager: EventRegistry,
  threadId: string,
  nodeId: string,
  timeout: number = 30000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;
  await eventManager.waitFor(
    "NODE_FAILED",
    actualTimeout,
    (event: { threadId?: string; nodeId?: string }) =>
      event.threadId === threadId && event.nodeId === nodeId,
  );
}
