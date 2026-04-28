/**
 * EventRegistry - Event Registry
 * Manages events during workflow execution, provides event listening and dispatching mechanism
 * Only supports global events, used for exposing workflow execution status externally
 *
 * Note: Internal event mechanism has been removed, replaced with direct method calls
 *
 * This module only exports class definition, not instances
 * Instances are managed uniformly through SingletonRegistry
 */

import type { BaseEvent, EventType, EventListener } from "@wf-agent/types";
import { ExecutionError, RuntimeValidationError } from "@wf-agent/types";
import { generateId } from "../../utils/index.js";
import { now, getErrorOrNew } from "@wf-agent/common-utils";

/**
 * Listener wrapper
 */
interface ListenerWrapper<T> {
  listener: (event: T) => void | Promise<void>;
  id: string;
  timestamp: number;
  priority: number;
  filter?: (event: T) => boolean;
  timeout?: number;
}

/**
 * EventRegistry - Event Registry
 *
 * Responsibilities:
 * - Global events: Exposed externally, users can listen (e.g., NODE_COMPLETED)
 * - Provides core functions like event registration, unregistration, triggering
 *
 * Design Principles:
 * - Only supports global events for workflow status notification
 * - Internal coordination uses direct method calls
 */
class EventRegistry {
  // Global event listeners (exposed externally)
  private globalListeners: Map<string, ListenerWrapper<unknown>[]> = new Map();

  /**
   * Register event listener (global event)
   * @param eventType Event type
   * @param listener Event listener
   * @param options Options (priority, filter, timeout, etc.)
   * @returns Unregister function
   */
  on<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options?: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
    },
  ): () => void {
    return this.registerGlobalListener(eventType, listener, options);
  }

  /**
   * Register global event listener
   * @param eventType Event type
   * @param listener Event listener
   * @param options Options
   * @returns Unregister function
   */
  private registerGlobalListener<T>(
    eventType: string,
    listener: (event: T) => void | Promise<void>,
    options?: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
    },
  ): () => void {
    // Validate parameters
    if (!eventType) {
      throw new RuntimeValidationError("EventType is required", { field: "eventType" });
    }
    if (typeof listener !== "function") {
      throw new RuntimeValidationError("Listener must be a function", { field: "listener" });
    }

    // Create listener wrapper
    const wrapper: ListenerWrapper<T> = {
      listener,
      id: generateId(),
      timestamp: now(),
      priority: options?.priority || 0,
      filter: options?.filter,
      timeout: options?.timeout,
    };

    // Add to global listeners list
    if (!this.globalListeners.has(eventType)) {
      this.globalListeners.set(eventType, []);
    }
    this.globalListeners.get(eventType)!.push(wrapper as ListenerWrapper<unknown>);

    // Sort by priority (higher priority first)
    this.globalListeners.get(eventType)!.sort((a, b) => b.priority - a.priority);

    // Return unregister function
    return () => this.unregisterGlobalListener(eventType, listener);
  }

  /**
   * Unregister event listener (global event)
   * @param eventType Event type
   * @param listener Event listener
   * @returns Whether successfully unregistered
   */
  off<T extends BaseEvent>(eventType: EventType, listener: EventListener<T>): boolean {
    return this.unregisterGlobalListener(eventType, listener);
  }

  /**
   * Unregister global event listener
   * @param eventType Event type
   * @param listener Event listener
   * @returns Whether successfully unregistered
   */
  private unregisterGlobalListener<T>(
    eventType: string,
    listener: (event: T) => void | Promise<void>,
  ): boolean {
    // Validate parameters
    if (!eventType) {
      throw new RuntimeValidationError("EventType is required", { field: "eventType" });
    }
    if (typeof listener !== "function") {
      throw new RuntimeValidationError("Listener must be a function", { field: "listener" });
    }

    // Get listeners array
    const wrappers = this.globalListeners.get(eventType);
    if (!wrappers) {
      return false;
    }

    // Find and remove listener
    const index = wrappers.findIndex(w => w.listener === listener);
    if (index === -1) {
      return false;
    }

    wrappers.splice(index, 1);

    // If array is empty, delete mapping
    if (wrappers.length === 0) {
      this.globalListeners.delete(eventType);
    }

    return true;
  }

  /**
   * Emit event
   * @param event Event object
   * @returns Promise that waits for all listeners to complete
   */
  async emit<T extends BaseEvent>(event: T): Promise<void> {
    // Validate event
    if (!event) {
      throw new RuntimeValidationError("Event is required", { field: "event" });
    }
    if (!event.type) {
      throw new RuntimeValidationError("Event type is required", { field: "event.type" });
    }

    // Get listeners
    const wrappers = this.globalListeners.get(event.type) || [];

    // Execute listeners
    for (const wrapper of wrappers) {
      // Check filter
      if (wrapper.filter && !wrapper.filter(event)) {
        continue;
      }

      try {
        // Execute listener (with timeout control)
        if (wrapper.timeout) {
          await Promise.race([
            wrapper.listener(event),
            new Promise((_, reject) =>
              setTimeout(
                () => reject(new Error(`Listener timeout after ${wrapper.timeout}ms`)),
                wrapper.timeout,
              ),
            ),
          ]);
        } else {
          await wrapper.listener(event);
        }
      } catch (error) {
        // Throw error, let caller decide how to handle
        throw new ExecutionError(
          "Event listener execution failed",
          undefined,
          undefined,
          {
            eventType: event.type,
            operation: "event_listener",
          },
          getErrorOrNew(error),
        );
      }
    }
  }

  /**
   * Register one-time event listener
   * @param eventType Event type
   * @param listener Event listener
   * @param options Options
   * @returns Unregister function
   */
  once<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options?: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
    },
  ): () => void {
    // Validate parameters
    if (!eventType) {
      throw new RuntimeValidationError("EventType is required", { field: "eventType" });
    }
    if (typeof listener !== "function") {
      throw new RuntimeValidationError("Listener must be a function", { field: "listener" });
    }

    // Create wrapper listener
    const wrapper: EventListener<T> = async (event: T) => {
      await listener(event);
      // Auto unregister
      this.off(eventType, wrapper);
    };

    // Register wrapper listener
    return this.on(eventType, wrapper, options);
  }

  /**
   * Wait for specific event to be emitted
   * @param eventType Event type
   * @param timeout Timeout (milliseconds)
   * @param filter Event filter function, only resolve Promise when returns true
   * @returns Promise that resolves to event object
   */
  waitFor<T extends BaseEvent>(
    eventType: EventType,
    timeout?: number,
    filter?: (event: T) => boolean,
  ): Promise<T> {
    return new Promise((resolve, reject) => {
      let timeoutId: NodeJS.Timeout | undefined;
      let resolved = false;

      // Create listener (don't use once, because filter may return false)
      const listener = (event: T) => {
        // Check filter
        if (filter && !filter(event)) {
          return; // Not matched, continue waiting
        }

        // Mark as resolved
        resolved = true;

        // Clean up resources
        if (timeoutId) {
          clearTimeout(timeoutId);
        }
        this.off(eventType, listener);

        // Resolve Promise
        resolve(event);
      };

      // Register listener
      this.on(eventType, listener);

      // Set timeout
      if (timeout) {
        timeoutId = setTimeout(() => {
          if (!resolved) {
            this.off(eventType, listener);
            reject(new Error(`Timeout waiting for event ${eventType}`));
          }
        }, timeout);
      }
    });
  }
}

/**
 * Export EventRegistry class
 */
export { EventRegistry };
