/**
 * EventRegistry - Event Registry
 * Manages events during workflow execution, provides event listening and dispatching mechanism
 * 
 * All event listeners must be execution-scoped:
 * - Register with executionId parameter (REQUIRED)
 * - Automatically cleaned up when execution ends
 * - Use case: Execution-specific business logic, UI updates
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
  failureCount: number;
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
  executionId: string; // Required execution ID for scoped listeners
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
  
  // Execution-scoped listener tracking (executionId -> Set of listener IDs)
  private executionScopedListeners: Map<string, Set<string>> = new Map();
  
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
   * Register event listener
   * 
   * @param eventType Event type
   * @param listener Event listener
   * @param options Options (priority, filter, timeout, executionId)
   *   - executionId is REQUIRED for all listeners
   * @returns Unregister function
   * 
   * @example
   * ```typescript
   * // Execution-scoped listener - only receives events for specific execution
   * const executionId = await sdk.startWorkflow(workflowId);
   * eventManager.on('NODE_COMPLETED', (event) => {
   *   console.log('Node completed:', event.nodeId);
   * }, { executionId });
   * ```
   */
  on<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string; // Required execution ID
    },
  ): () => void {
    return this.registerExecutionScopedListener(eventType, listener, options);
  }

  /**
   * Register execution-scoped event listener
   * @param eventType Event type
   * @param listener Event listener
   * @param options Options (executionId is required)
   * @returns Unregister function
   */
  private registerExecutionScopedListener<T>(
    eventType: string,
    listener: (event: T) => void | Promise<void>,
    options: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string; // Required
    },
  ): () => void {
    // Validate parameters
    if (!eventType) {
      throw new RuntimeValidationError("EventType is required", { field: "eventType" });
    }
    if (typeof listener !== "function") {
      throw new RuntimeValidationError("Listener must be a function", { field: "listener" });
    }
    if (!options.executionId) {
      throw new RuntimeValidationError(
        "executionId is required for event subscriptions",
        { field: "options.executionId" }
      );
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
      priority: options.priority || 0,
      filter: options.filter,
      timeout: options.timeout ?? this.config.defaultListenerTimeout,
      executionId: options.executionId,
    };

    // Add to global listeners list
    if (!this.globalListeners.has(eventType)) {
      this.globalListeners.set(eventType, []);
    }
    this.globalListeners.get(eventType)!.push(wrapper as ListenerWrapper<unknown>);

    // Sort by priority (higher priority first)
    this.globalListeners.get(eventType)!.sort((a, b) => b.priority - a.priority);

    // Track execution association
    if (!this.executionScopedListeners.has(options.executionId)) {
      this.executionScopedListeners.set(options.executionId, new Set());
    }
    this.executionScopedListeners.get(options.executionId)!.add(wrapper.id);

    // Initialize metrics for this listener
    this.listenerMetrics.set(wrapper.id, {
      totalExecutions: 0,
      totalDuration: 0,
      averageDuration: 0,
      lastExecutionTime: 0,
      slowExecutionCount: 0,
      failureCount: 0,
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

    // Get wrapper before removing to clean up metrics
    const wrapper = wrappers[index];
    if (!wrapper) {
      return false;
    }
    
    wrappers.splice(index, 1);

    // Clean up listener metrics to prevent memory leak
    this.listenerMetrics.delete(wrapper.id);
    
    // Clean up execution tracking if applicable
    if (wrapper.executionId) {
      const listenerIds = this.executionScopedListeners.get(wrapper.executionId);
      if (listenerIds) {
        listenerIds.delete(wrapper.id);
        // Remove empty sets
        if (listenerIds.size === 0) {
          this.executionScopedListeners.delete(wrapper.executionId);
        }
      }
    }

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
    
    if (failed) {
      metrics.failureCount++;
    }
    
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
   * @param options Options (priority, filter, timeout, executionId) - executionId is required
   * @returns Unregister function
   */
  once<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string; // Required execution ID
    },
  ): () => void {
    // Validate parameters
    if (!eventType) {
      throw new RuntimeValidationError("EventType is required", { field: "eventType" });
    }
    if (typeof listener !== "function") {
      throw new RuntimeValidationError("Listener must be a function", { field: "listener" });
    }
    if (!options.executionId) {
      throw new RuntimeValidationError(
        "executionId is required for event subscriptions",
        { field: "options.executionId" }
      );
    }

    // Create wrapper listener
    const wrapper: EventListener<T> = async (event: T) => {
      await listener(event);
      // Auto unregister
      this.off(eventType, wrapper);
    };

    // Register wrapper listener with same options
    return this.on(eventType, wrapper, options);
  }

  /**
   * Wait for specific event to be emitted
   * @param eventType Event type
   * @param executionId Execution ID (required)
   * @param timeout Timeout (milliseconds)
   * @param filter Event filter function, only resolve Promise when returns true
   * @returns Promise that resolves to event object
   */
  waitFor<T extends BaseEvent>(
    eventType: EventType,
    executionId: string,
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

      // Register listener with executionId
      this.on(eventType, listener, { executionId });

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

  /**
   * Cleanup all listeners associated with a specific execution
   * Should be called when execution completes (success, failure, or cancellation)
   * 
   * @param executionId Execution ID
   * @returns Number of listeners cleaned up
   * 
   * @example
   * ```typescript
   * // In workflow lifecycle coordinator
   * async stopWorkflowExecution(executionId: string): Promise<void> {
   *   // ... other cleanup logic ...
   *   const cleanedCount = eventRegistry.cleanupExecutionListeners(executionId);
   *   logger.info('Cleaned up event listeners', { executionId, cleanedCount });
   * }
   * ```
   */
  cleanupExecutionListeners(executionId: string): number {
    if (!executionId) {
      logger.warn('cleanupExecutionListeners called with empty executionId');
      return 0;
    }

    const listenerIds = this.executionScopedListeners.get(executionId);
    if (!listenerIds || listenerIds.size === 0) {
      logger.debug('No execution-scoped listeners to clean up', { executionId });
      return 0;
    }

    let cleanedCount = 0;

    // Find and remove all listeners for this execution
    for (const [eventType, wrappers] of this.globalListeners.entries()) {
      // Filter out listeners belonging to this execution
      const toRemove = wrappers.filter(w => listenerIds.has(w.id));
      
      for (const wrapper of toRemove) {
        const index = wrappers.indexOf(wrapper);
        if (index !== -1) {
          wrappers.splice(index, 1);
          this.listenerMetrics.delete(wrapper.id);
          cleanedCount++;
        }
      }
      
      // Remove empty arrays
      if (wrappers.length === 0) {
        this.globalListeners.delete(eventType);
      }
    }

    // Remove execution tracking
    this.executionScopedListeners.delete(executionId);

    logger.info('Cleaned up execution-scoped listeners', {
      executionId,
      cleanedCount,
    });

    return cleanedCount;
  }

  /**
   * Get statistics for execution-scoped listeners
   * Useful for debugging and monitoring listener lifecycle
   * 
   * @returns Map of execution ID to listener count
   * 
   * @example
   * ```typescript
   * const stats = eventRegistry.getExecutionListenerStats();
   * for (const [executionId, count] of stats) {
   *   console.log(`Execution ${executionId} has ${count} active listeners`);
   * }
   * ```
   */
  getExecutionListenerStats(): Map<string, number> {
    const stats = new Map<string, number>();
    
    for (const [executionId, listenerIds] of this.executionScopedListeners.entries()) {
      stats.set(executionId, listenerIds.size);
    }
    
    return stats;
  }

  /**
   * Get detailed information about all active listeners
   * Useful for debugging memory leaks and listener management issues
   * 
   * @returns Array of listener information
   * 
   * @example
   * ```typescript
   * const listeners = eventRegistry.getAllListenerInfo();
   * const executionScoped = listeners.filter(l => l.executionId !== undefined);
   * console.log(`Found ${executionScoped.length} execution-scoped listeners`);
   * ```
   */
  getAllListenerInfo(): Array<{
    id: string;
    eventType: string;
    executionId?: string;
    priority: number;
    registeredAt: number;
    metrics?: ListenerMetrics;
  }> {
    const result: Array<{
      id: string;
      eventType: string;
      executionId?: string;
      priority: number;
      registeredAt: number;
      metrics?: ListenerMetrics;
    }> = [];

    for (const [eventType, wrappers] of this.globalListeners.entries()) {
      for (const wrapper of wrappers) {
        result.push({
          id: wrapper.id,
          eventType,
          executionId: wrapper.executionId,
          priority: wrapper.priority,
          registeredAt: wrapper.timestamp,
          metrics: this.listenerMetrics.get(wrapper.id),
        });
      }
    }

    return result;
  }

  /**
   * Get listeners count by event type
   * 
   * @returns Map of event type to listener count
   */
  getListenerCountByEventType(): Map<string, { total: number; executionScoped: number; global: number }> {
    const result = new Map<string, { total: number; executionScoped: number; global: number }>();

    for (const [eventType, wrappers] of this.globalListeners.entries()) {
      let executionScoped = 0;
      let global = 0;

      for (const wrapper of wrappers) {
        if (wrapper.executionId) {
          executionScoped++;
        } else {
          global++;
        }
      }

      result.set(eventType, {
        total: wrappers.length,
        executionScoped,
        global,
      });
    }

    return result;
  }
}

/**
 * Export EventRegistry class
 */
export { EventRegistry };
