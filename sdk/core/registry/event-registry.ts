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
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "EventRegistry" });

/**
 * EventRegistry configuration options
 */
export interface EventRegistryConfig {
  /** Maximum number of listeners per event type (prevents memory overflow) */
  maxListenersPerEvent?: number;
  /** Default listener timeout in milliseconds */
  defaultListenerTimeout?: number;
  /** Slow listener threshold in milliseconds (for warning logs) */
  slowListenerThreshold?: number;
  /** Enable backpressure control */
  enableBackpressure?: boolean;
}

/**
 * Listener performance metrics
 */
interface ListenerMetrics {
  totalExecutions: number;
  totalDuration: number;
  averageDuration: number;
  lastExecutionTime: number;
  slowExecutionCount: number;
}

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
 * - Includes backpressure control and performance monitoring
 */
class EventRegistry {
  // Global event listeners (exposed externally)
  private globalListeners: Map<string, ListenerWrapper<unknown>[]> = new Map();
  
  // Configuration
  private config: Required<EventRegistryConfig>;
  
  // Listener performance metrics
  private listenerMetrics: Map<string, ListenerMetrics> = new Map();

  constructor(config?: EventRegistryConfig) {
    this.config = {
      maxListenersPerEvent: config?.maxListenersPerEvent ?? 100,
      defaultListenerTimeout: config?.defaultListenerTimeout ?? 30000, // 30 seconds
      slowListenerThreshold: config?.slowListenerThreshold ?? 5000, // 5 seconds
      enableBackpressure: config?.enableBackpressure ?? true,
    };
  }

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

    // Backpressure control: check listener count
    if (this.config.enableBackpressure) {
      const currentListeners = this.globalListeners.get(eventType) || [];
      if (currentListeners.length >= this.config.maxListenersPerEvent) {
        logger.warn(`Maximum listeners reached for event type '${eventType}'`, {
          eventType,
          currentCount: currentListeners.length,
          maxAllowed: this.config.maxListenersPerEvent,
        });
        throw new RuntimeValidationError(
          `Maximum listeners (${this.config.maxListenersPerEvent}) reached for event type '${eventType}'`,
          { field: "eventType" },
        );
      }
    }

    // Create listener wrapper
    const wrapper: ListenerWrapper<T> = {
      listener,
      id: generateId(),
      timestamp: now(),
      priority: options?.priority || 0,
      filter: options?.filter,
      timeout: options?.timeout ?? this.config.defaultListenerTimeout,
    };

    // Add to global listeners list
    if (!this.globalListeners.has(eventType)) {
      this.globalListeners.set(eventType, []);
    }
    this.globalListeners.get(eventType)!.push(wrapper as ListenerWrapper<unknown>);

    // Sort by priority (higher priority first)
    this.globalListeners.get(eventType)!.sort((a, b) => b.priority - a.priority);

    // Initialize metrics for this listener
    this.listenerMetrics.set(wrapper.id, {
      totalExecutions: 0,
      totalDuration: 0,
      averageDuration: 0,
      lastExecutionTime: 0,
      slowExecutionCount: 0,
    });

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

      const startTime = now();
      try {
        // Execute listener (with timeout control)
        const timeout = wrapper.timeout ?? this.config.defaultListenerTimeout;
        await Promise.race([
          wrapper.listener(event),
          new Promise((_, reject) =>
            setTimeout(
              () => reject(new Error(`Listener timeout after ${timeout}ms`)),
              timeout,
            ),
          ),
        ]);
        
        // Track performance metrics
        const duration = now() - startTime;
        this.updateListenerMetrics(wrapper.id, duration);
        
        // Log slow listener warning
        if (duration > this.config.slowListenerThreshold) {
          logger.warn('Slow event listener detected', {
            listenerId: wrapper.id,
            duration,
            eventType: event.type,
            threshold: this.config.slowListenerThreshold,
          });
        }
      } catch (error) {
        // Track failed execution
        this.updateListenerMetrics(wrapper.id, now() - startTime, true);
        
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
   * Update listener performance metrics
   * @param listenerId Listener ID
   * @param duration Execution duration in milliseconds
   * @param failed Whether the execution failed
   */
  private updateListenerMetrics(listenerId: string, duration: number, failed: boolean = false): void {
    const metrics = this.listenerMetrics.get(listenerId);
    if (!metrics) return;

    metrics.totalExecutions++;
    metrics.totalDuration += duration;
    metrics.averageDuration = metrics.totalDuration / metrics.totalExecutions;
    metrics.lastExecutionTime = now();
    
    if (duration > this.config.slowListenerThreshold) {
      metrics.slowExecutionCount++;
    }
  }

  /**
   * Get performance metrics for a specific listener
   * @param listenerId Listener ID
   * @returns Listener metrics or undefined if not found
   */
  getListenerMetrics(listenerId: string): ListenerMetrics | undefined {
    return this.listenerMetrics.get(listenerId);
  }

  /**
   * Get all listener metrics
   * @returns Map of listener ID to metrics
   */
  getAllListenerMetrics(): Map<string, ListenerMetrics> {
    return new Map(this.listenerMetrics);
  }

  /**
   * Clear all listener metrics
   */
  clearListenerMetrics(): void {
    this.listenerMetrics.clear();
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
