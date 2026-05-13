/**
 * ExecutionEventEmitter - Per-Execution Event Emitter
 * 
 * Provides isolated event management for a single workflow execution.
 * Each execution gets its own ExecutionEventEmitter instance, eliminating the need
 * to pass executionId parameters everywhere.
 * 
 * Design Principles:
 * - One emitter per execution (isolated state)
 * - No cross-execution events (by design)
 * - Automatic cleanup when execution ends
 * - Simplified API (no executionId parameter needed)
 */

import type { BaseEvent, EventType, EventListener } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { now, getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "EventEmitter" });

/**
 * Listener wrapper within an EventEmitter
 */
interface ListenerWrapper<T> {
  listener: (event: T) => void | Promise<void>;
  id: string;
  timestamp: number;
  filter?: (event: T) => boolean;
  timeout?: number;
}

/**
 * Metrics for a specific listener
 */
interface ListenerMetrics {
  totalExecutions: number;
  totalDuration: number;
  failureCount: number;
  slowExecutionCount: number; // Executions exceeding timeout
}

/**
 * Options for registering event listeners
 */
export interface EventEmitterOptions<T extends BaseEvent = BaseEvent> {
  filter?: (event: T) => boolean;
  timeout?: number;
}

/**
 * ExecutionEventEmitter - Manages events for a single execution
 * 
 * @example
 * ```typescript
 * const emitter = new ExecutionEventEmitter('exec-123');
 * 
 * // Register listener (no executionId needed!)
 * emitter.on('NODE_COMPLETED', (event) => {
 *   console.log('Node completed:', event.nodeId);
 * });
 * 
 * // Emit event
 * await emitter.emit({ type: 'NODE_COMPLETED', nodeId: 'node-1' });
 * 
 * // Cleanup all listeners
 * emitter.removeAllListeners();
 * ```
 */
export class ExecutionEventEmitter {
  /** Execution ID this emitter belongs to */
  public readonly executionId: string;
  
  private listeners: Map<string, ListenerWrapper<any>[]> = new Map();
  private metrics: Map<string, Map<string, ListenerMetrics>> = new Map();
  private isDisposed: boolean = false;

  constructor(executionId: string) {
    if (!executionId) {
      throw new RuntimeValidationError("Execution ID is required", { field: "executionId" });
    }
    this.executionId = executionId;
  }

  /**
   * Register event listener
   * 
   * @param eventType Event type
   * @param listener Event listener function
   * @param options Optional filter and timeout
   * @returns Unsubscribe function
   * 
   * @example
   * ```typescript
   * emitter.on('NODE_STARTED', (event) => {
   *   console.log('Node started:', event.nodeId);
   * });
   * 
   * // With filter
   * emitter.on('NODE_COMPLETED', 
   *   (event) => console.log('LLM node done'),
   *   { filter: (e) => e.nodeType === 'llm' }
   * );
   * ```
   */
  on<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options?: EventEmitterOptions<T>,
  ): () => void {
    this.validateNotDisposed();

    const wrapper: ListenerWrapper<T> = {
      listener,
      id: `${eventType}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      timestamp: now(),
      filter: options?.filter,
      timeout: options?.timeout,
    };

    if (!this.listeners.has(eventType)) {
      this.listeners.set(eventType, []);
    }

    this.listeners.get(eventType)!.push(wrapper);

    // Initialize metrics
    if (!this.metrics.has(eventType)) {
      this.metrics.set(eventType, new Map());
    }
    this.metrics.get(eventType)!.set(wrapper.id, {
      totalExecutions: 0,
      totalDuration: 0,
      failureCount: 0,
      slowExecutionCount: 0,
    });

    logger.debug('Listener registered', {
      executionId: this.executionId,
      eventType,
      listenerId: wrapper.id,
    });

    // Return unsubscribe function
    return () => {
      this.off(eventType, wrapper.id);
    };
  }

  /**
   * Register one-time event listener (automatically removed after first trigger)
   * 
   * @param eventType Event type
   * @param listener Event listener function
   * @param options Optional filter and timeout
   * @returns Unsubscribe function
   * 
   * @example
   * ```typescript
   * emitter.once('WORKFLOW_COMPLETED', (event) => {
   *   console.log('Workflow finished!');
   * });
   * ```
   */
  once<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options?: EventEmitterOptions<T>,
  ): () => void {
    let unsubscribe: (() => void) | null = null;

    const wrappedListener = async (event: T) => {
      try {
        await listener(event);
      } finally {
        // Automatically unsubscribe after first execution
        if (unsubscribe) {
          unsubscribe();
        }
      }
    };

    unsubscribe = this.on(eventType, wrappedListener as EventListener<T>, options);
    return unsubscribe;
  }

  /**
   * Wait for a specific event (returns a promise)
   * 
   * @param eventType Event type to wait for
   * @param timeout Timeout in milliseconds (optional)
   * @param filter Optional filter function
   * @returns Promise that resolves with the event
   * 
   * @example
   * ```typescript
   * // Wait for next node completion
   * const event = await emitter.waitFor('NODE_COMPLETED', { timeout: 5000 });
   * console.log('Completed node:', event.nodeId);
   * ```
   */
  waitFor<T extends BaseEvent>(
    eventType: EventType,
    options?: { timeout?: number; filter?: (event: T) => boolean },
  ): Promise<T> {
    return new Promise((resolve, reject) => {
      let timeoutId: NodeJS.Timeout | undefined;

      const unsubscribe = this.once(eventType, (event: T) => {
        if (timeoutId) {
          clearTimeout(timeoutId);
        }
        resolve(event);
      }, {
        filter: options?.filter,
      });

      // Set timeout if specified
      if (options?.timeout) {
        timeoutId = setTimeout(() => {
          unsubscribe();
          reject(new Error(`Timeout waiting for event: ${eventType}`));
        }, options.timeout);
      }
    });
  }

  /**
   * Remove a specific listener
   * 
   * @param eventType Event type
   * @param listenerId Listener ID (from registration)
   */
  off(eventType: EventType, listenerId: string): void {
    const wrappers = this.listeners.get(eventType);
    if (!wrappers) {
      return;
    }

    const index = wrappers.findIndex(w => w.id === listenerId);
    if (index !== -1) {
      wrappers.splice(index, 1);
      
      // Clean up metrics
      const eventMetrics = this.metrics.get(eventType);
      if (eventMetrics) {
        eventMetrics.delete(listenerId);
      }

      logger.debug('Listener removed', {
        executionId: this.executionId,
        eventType,
        listenerId,
      });
    }
  }

  /**
   * Emit event to all registered listeners
   * 
   * @param event Event object
   * @returns Promise that waits for all listeners to complete
   * 
   * @example
   * ```typescript
   * await emitter.emit({
   *   type: 'NODE_COMPLETED',
   *   nodeId: 'node-1',
   *   executionId: 'exec-123',
   * });
   * ```
   */
  async emit<T extends BaseEvent>(event: T): Promise<void> {
    this.validateNotDisposed();

    // Validate event
    if (!event) {
      throw new RuntimeValidationError("Event is required", { field: "event" });
    }
    if (!event.type) {
      throw new RuntimeValidationError("Event type is required", { field: "event.type" });
    }

    const wrappers = this.listeners.get(event.type) || [];

    if (wrappers.length === 0) {
      logger.debug('No listeners for event', {
        executionId: this.executionId,
        eventType: event.type,
      });
      return;
    }

    logger.debug('Emitting event', {
      executionId: this.executionId,
      eventType: event.type,
      listenerCount: wrappers.length,
    });

    // Execute all listeners
    const errors: Error[] = [];
    
    for (const wrapper of wrappers) {
      // Check filter
      if (wrapper.filter && !wrapper.filter(event)) {
        continue;
      }

      const startTime = now();
      const metrics = this.metrics.get(event.type)?.get(wrapper.id);

      try {
        await wrapper.listener(event);

        // Update metrics on success
        if (metrics) {
          metrics.totalExecutions++;
          metrics.totalDuration += now() - startTime;

          // Check if execution was slow
          if (wrapper.timeout && (now() - startTime) > wrapper.timeout) {
            metrics.slowExecutionCount++;
          }
        }
      } catch (error) {
        const err = getErrorOrNew(error);
        errors.push(err);

        // Update metrics on failure
        if (metrics) {
          metrics.failureCount++;
        }

        logger.error('Listener execution failed', {
          executionId: this.executionId,
          eventType: event.type,
          listenerId: wrapper.id,
          error: err.message,
        });
      }
    }

    // If any listeners failed, throw aggregated error
    if (errors.length > 0) {
      const errorMessage = `${errors.length} listener(s) failed for event ${event.type}`;
      const aggregatedError = new Error(errorMessage);
      (aggregatedError as any).causes = errors;
      throw aggregatedError;
    }
  }

  /**
   * Remove all listeners for this emitter
   * Should be called when execution completes
   */
  removeAllListeners(): void {
    this.listeners.clear();
    this.metrics.clear();
    this.isDisposed = true;

    logger.info('All listeners removed', {
      executionId: this.executionId,
    });
  }

  /**
   * Get listener count by event type
   * @returns Map of event type to listener count
   */
  getListenerCount(): Map<string, number> {
    const counts = new Map<string, number>();
    
    for (const [eventType, wrappers] of this.listeners.entries()) {
      counts.set(eventType, wrappers.length);
    }

    return counts;
  }

  /**
   * Get metrics for a specific listener
   * @param eventType Event type
   * @param listenerId Listener ID
   * @returns Listener metrics or undefined
   */
  getListenerMetrics(eventType: string, listenerId: string): ListenerMetrics | undefined {
    return this.metrics.get(eventType)?.get(listenerId);
  }

  /**
   * Get all listener information for debugging
   * @returns Array of listener info objects
   */
  getAllListenerInfo(): Array<{
    id: string;
    eventType: string;
    registeredAt: number;
    metrics?: {
      totalExecutions: number;
      averageDuration: number;
      failureCount: number;
      slowExecutionCount: number;
    };
  }> {
    const result: Array<{
      id: string;
      eventType: string;
      registeredAt: number;
      metrics?: {
        totalExecutions: number;
        averageDuration: number;
        failureCount: number;
        slowExecutionCount: number;
      };
    }> = [];

    for (const [eventType, wrappers] of this.listeners.entries()) {
      for (const wrapper of wrappers) {
        const metrics = this.metrics.get(eventType)?.get(wrapper.id);
        
        result.push({
          id: wrapper.id,
          eventType,
          registeredAt: wrapper.timestamp,
          metrics: metrics ? {
            totalExecutions: metrics.totalExecutions,
            averageDuration: metrics.totalExecutions > 0
              ? metrics.totalDuration / metrics.totalExecutions
              : 0,
            failureCount: metrics.failureCount,
            slowExecutionCount: metrics.slowExecutionCount,
          } : undefined,
        });
      }
    }

    return result;
  }

  /**
   * Check if emitter has been disposed
   */
  isEmitterDisposed(): boolean {
    return this.isDisposed;
  }

  /**
   * Validate that emitter is not disposed
   * @throws Error if emitter is disposed
   */
  private validateNotDisposed(): void {
    if (this.isDisposed) {
      throw new Error(
        `EventEmitter for execution ${this.executionId} has been disposed. ` +
        'Cannot register listeners or emit events after disposal.'
      );
    }
  }
}
