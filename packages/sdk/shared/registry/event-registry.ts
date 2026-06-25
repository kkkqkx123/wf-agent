/**
 * EventRegistry - Event Registry
 *
 * Merged from previously separate event-emitter.ts and event-registry.ts.
 * ExecutionEventEmitter is now an internal implementation detail within this file.
 *
 * Manages events during workflow execution, provides event listening and dispatching mechanism.
 *
 * Design Principles:
 * - One emitter per execution (isolated state)
 * - No cross-execution events (by design)
 * - Automatic cleanup when execution ends
 * - Simplified API (no executionId parameter needed when using emitter directly)
 * - Global listeners for cross-execution monitoring
 */

import type { BaseEvent, EventType, EventListener } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { now, getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import {
  EventMetricsCollector,
  type AggregatedEventStat,
  type EventMetricsSummary,
  type EventMetricLabels,
} from "../../metrics/event-collector.js";

const logger = createContextualLogger({ operation: "EventRegistry" });

// ==================== Internal Types ====================

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

// ==================== ExecutionEventEmitter (internal) ====================

/**
 * ExecutionEventEmitter - Per-Execution Event Emitter
 *
 * Provides isolated event management for a single workflow execution.
 * Each execution gets its own ExecutionEventEmitter instance, eliminating the need
 * to pass executionId parameters everywhere.
 *
 * @internal This class is an internal implementation detail of EventRegistry.
 * External consumers should use EventRegistry.getEmitter() to obtain an instance.
 */
export class ExecutionEventEmitter {
  /** Execution ID this emitter belongs to */
  public readonly executionId: string;

  private listeners: Map<string, Array<ListenerWrapper<unknown>>> = new Map();
  private metrics: Map<string, Map<string, ListenerMetrics>> = new Map();
  private isDisposed: boolean = false;

  // Event batching support
  private batchDepth: number = 0;
  private batchBuffer: Array<BaseEvent> = [];

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

    (this.listeners.get(eventType)! as Array<ListenerWrapper<T>>).push(wrapper);

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

    logger.debug("Listener registered", {
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
   */
  waitFor<T extends BaseEvent>(
    eventType: EventType,
    options?: { timeout?: number; filter?: (event: T) => boolean },
  ): Promise<T> {
    return new Promise((resolve, reject) => {
      let timeoutId: NodeJS.Timeout | undefined;

      const unsubscribe = this.once(
        eventType,
        (event: T) => {
          if (timeoutId) {
            clearTimeout(timeoutId);
          }
          resolve(event);
        },
        {
          filter: options?.filter,
        },
      );

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
   */
  off(eventType: EventType, listenerId: string): void {
    const wrappers = this.listeners.get(eventType);
    if (!wrappers) {
      return;
    }

    const index = wrappers.findIndex(w => w.id === listenerId);
    if (index !== -1) {
      wrappers.splice(index, 1);

      const eventMetrics = this.metrics.get(eventType);
      if (eventMetrics) {
        eventMetrics.delete(listenerId);
      }

      logger.debug("Listener removed", {
        executionId: this.executionId,
        eventType,
        listenerId,
      });
    }
  }

  /**
   * Internal: emit event directly to all registered listeners (bypasses batching)
   */
  private async emitNow<T extends BaseEvent>(event: T): Promise<void> {
    this.validateNotDisposed();

    if (!event) {
      throw new RuntimeValidationError("Event is required", { field: "event" });
    }
    if (!event.type) {
      throw new RuntimeValidationError("Event type is required", { field: "event.type" });
    }

    const wrappers = this.listeners.get(event.type) || [];

    if (wrappers.length === 0) {
      logger.debug("No listeners for event", {
        executionId: this.executionId,
        eventType: event.type,
      });
      return;
    }

    logger.debug("Emitting event", {
      executionId: this.executionId,
      eventType: event.type,
      listenerCount: wrappers.length,
    });

    const errors: Error[] = [];

    for (const wrapper of wrappers) {
      if (wrapper.filter && !wrapper.filter(event)) {
        continue;
      }

      const startTime = now();
      const metrics = this.metrics.get(event.type)?.get(wrapper.id);

      try {
        await wrapper.listener(event);

        if (metrics) {
          metrics.totalExecutions++;
          metrics.totalDuration += now() - startTime;

          if (wrapper.timeout && now() - startTime > wrapper.timeout) {
            metrics.slowExecutionCount++;
          }
        }
      } catch (error) {
        const err = getErrorOrNew(error);
        errors.push(err);

        if (metrics) {
          metrics.failureCount++;
        }

        logger.error("Listener execution failed", {
          executionId: this.executionId,
          eventType: event.type,
          listenerId: wrapper.id,
          error: err.message,
        });
      }
    }

    if (errors.length > 0) {
      const errorMessage = `${errors.length} listener(s) failed for event ${event.type}`;
      const aggregatedError = new Error(errorMessage);
      (aggregatedError as Error & { causes: Error[] }).causes = errors;
      throw aggregatedError;
    }
  }

  /**
   * Emit event to all registered listeners.
   * If batching is active, buffers the event and flushes on endBatch().
   */
  async emit<T extends BaseEvent>(event: T): Promise<void> {
    if (this.batchDepth > 0) {
      this.batchBuffer.push(event);
      return;
    }
    await this.emitNow(event);
  }

  /**
   * Remove all listeners for this emitter
   */
  removeAllListeners(): void {
    this.listeners.clear();
    this.metrics.clear();
    this.isDisposed = true;

    logger.info("All listeners removed", {
      executionId: this.executionId,
    });
  }

  /**
   * Get listener count by event type
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
   */
  getListenerMetrics(eventType: string, listenerId: string): ListenerMetrics | undefined {
    return this.metrics.get(eventType)?.get(listenerId);
  }

  /**
   * Get all listener information for debugging
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
          metrics: metrics
            ? {
                totalExecutions: metrics.totalExecutions,
                averageDuration:
                  metrics.totalExecutions > 0 ? metrics.totalDuration / metrics.totalExecutions : 0,
                failureCount: metrics.failureCount,
                slowExecutionCount: metrics.slowExecutionCount,
              }
            : undefined,
        });
      }
    }

    return result;
  }

  /**
   * Register a listener for checkpoint events with entity type and checkpoint type filtering.
   *
   * @param entityType Optional entity type filter ('workflow', 'agent', etc.)
   * @param checkpointType Optional checkpoint type filter ('FULL', 'DELTA')
   * @param listener Event listener
   * @param options Listener options (filter, timeout)
   * @returns Unsubscribe function
   */
  onCheckpointEvent<T extends BaseEvent>(
    eventType: EventType,
    entityType: string | undefined,
    checkpointType: string | undefined,
    listener: EventListener<T>,
    options?: EventEmitterOptions<T>,
  ): () => void {
    const combinedFilter = (event: T): boolean => {
      if (options?.filter && !options.filter(event)) return false;
      if (entityType && event.metadata && (event.metadata as Record<string, unknown>)['entityType'] !== entityType) return false;
      if (checkpointType && event.metadata && (event.metadata as Record<string, unknown>)['checkpointType'] !== checkpointType) return false;
      return true;
    };

    return this.on(eventType, listener, { ...options, filter: combinedFilter });
  }

  /**
   * Begin a batch operation.
   * While batching, emitted events are buffered and flushed on endBatch().
   * Supports nested batch calls (reference counted).
   */
  beginBatch(): void {
    this.validateNotDisposed();
    this.batchDepth++;
    logger.debug("Batch started", {
      executionId: this.executionId,
      depth: this.batchDepth,
    });
  }

  /**
   * End a batch operation.
   * Flushes buffered events when the outermost batch ends.
   */
  async endBatch(): Promise<void> {
    this.validateNotDisposed();

    if (this.batchDepth <= 0) {
      logger.warn("endBatch called without matching beginBatch", {
        executionId: this.executionId,
      });
      return;
    }

    this.batchDepth--;

    if (this.batchDepth === 0 && this.batchBuffer.length > 0) {
      const buffer = this.batchBuffer;
      this.batchBuffer = [];

      logger.debug("Flushing batch buffer", {
        executionId: this.executionId,
        eventCount: buffer.length,
      });

      // Re-emit all buffered events individually
      for (const event of buffer) {
        await this.emitNow(event);
      }
    }

    logger.debug("Batch ended", {
      executionId: this.executionId,
      depth: this.batchDepth,
    });
  }

  /**
   * Check if emitter has been disposed
   */
  isEmitterDisposed(): boolean {
    return this.isDisposed;
  }

  private validateNotDisposed(): void {
    if (this.isDisposed) {
      throw new RuntimeValidationError("Event emitter has been disposed", {
        context: { executionId: this.executionId },
      });
    }
  }
}

// ==================== EventRegistry ====================

/**
 * EventRegistry - Event Registry
 *
 * Responsibilities:
 * - Manage per-execution EventEmitter instances
 * - Provide centralized event management with execution isolation
 * - Aggregate cross-execution metrics
 *
 * Design Principles:
 * - Each execution gets its own EventEmitter instance
 * - No executionId parameter needed when using emitter directly
 * - Cross-execution statistics use metrics aggregation
 */
class EventRegistry {
  // Per-execution ExecutionEventEmitter instances
  private emitters: Map<string, ExecutionEventEmitter> = new Map();

  // Cross-execution metrics collector (new universal metrics system)
  private metricsCollector: EventMetricsCollector;

  // Global event listeners (for cross-execution monitoring like EventResourceAPI)
  private globalListeners: Array<(event: BaseEvent) => void | Promise<void>> = [];

  constructor() {
    this.metricsCollector = new EventMetricsCollector();
  }

  /**
   * Register a global event listener that receives all events across all executions
   *
   * @param listener Global event listener function
   * @returns Unsubscribe function
   */
  onGlobal(listener: (event: BaseEvent) => void | Promise<void>): () => void {
    this.globalListeners.push(listener);

    logger.debug("Global event listener registered", {
      totalGlobalListeners: this.globalListeners.length,
    });

    return () => {
      const index = this.globalListeners.indexOf(listener);
      if (index !== -1) {
        this.globalListeners.splice(index, 1);
        logger.debug("Global event listener removed", {
          remainingListeners: this.globalListeners.length,
        });
      }
    };
  }

  /**
   * Get or create ExecutionEventEmitter for a specific execution
   *
   * This is the preferred API for event management. Each execution gets its own
   * isolated emitter, eliminating the need to pass executionId parameters.
   *
   * @param executionId Execution ID
   * @returns ExecutionEventEmitter instance for the execution
   */
  getEmitter(executionId: string): ExecutionEventEmitter {
    if (!executionId) {
      throw new RuntimeValidationError("Execution ID is required", { field: "executionId" });
    }

    if (!this.emitters.has(executionId)) {
      logger.debug("Creating new ExecutionEventEmitter", { executionId });
      this.emitters.set(executionId, new ExecutionEventEmitter(executionId));
    }

    return this.emitters.get(executionId)!;
  }

  /**
   * Register event listener (delegates to EventEmitter)
   *
   * @param eventType Event type
   * @param listener Event listener
   * @param options Options (filter, timeout, executionId) - executionId is required
   * @returns Unregister function
   */
  on<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options: {
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string;
    },
  ): () => void {
    const emitter = this.getEmitter(options.executionId);
    return emitter.on(eventType, listener, {
      filter: options.filter,
      timeout: options.timeout,
    });
  }

  /**
   * Wait for specific event to be emitted
   *
   * @param eventType Event type
   * @param executionId Execution ID (required)
   * @param timeout Timeout in milliseconds
   * @param filter Optional filter function
   * @returns Promise that resolves with the event
   */
  waitFor<T extends BaseEvent>(
    eventType: EventType,
    executionId: string,
    timeout?: number,
    filter?: (event: T) => boolean,
  ): Promise<T> {
    const emitter = this.getEmitter(executionId);
    return emitter.waitFor(eventType, { timeout, filter });
  }

  /**
   * Emit event
   *
   * @param event Event object
   * @returns Promise that waits for all listeners to complete
   */
  async emit<T extends BaseEvent>(event: T): Promise<void> {
    if (!event.executionId) {
      throw new RuntimeValidationError("Event must have executionId", {
        field: "event.executionId",
      });
    }

    const emitter = this.getEmitter(event.executionId);
    await emitter.emit(event);

    // Record event metrics with dimensional labels for aggregation
    const labels: EventMetricLabels = {
      workflow_id: event.workflowId,
      agent_loop_id: (event as any).agentLoopId, // Support agent events with agentLoopId
    };

    // Remove undefined labels to avoid polluting metrics data
    Object.keys(labels).forEach(key => {
      if (labels[key] === undefined) {
        delete labels[key];
      }
    });

    this.metricsCollector.recordEvent(event.type, event.executionId, labels);

    // Notify global listeners after successful emission
    if (this.globalListeners.length > 0) {
      const errors: Error[] = [];
      for (const listener of this.globalListeners) {
        try {
          await listener(event);
        } catch (error) {
          errors.push(getErrorOrNew(error));
          logger.error("Global listener execution failed", {
            error: getErrorOrNew(error).message,
          });
        }
      }

      if (errors.length > 0) {
        logger.warn(`${errors.length} global listener(s) failed`, {
          totalErrors: errors.length,
        });
      }
    }
  }

  /**
   * Register one-time event listener
   *
   * @param eventType Event type
   * @param listener Event listener
   * @param options Options (filter, timeout, executionId) - executionId is required
   * @returns Unregister function
   */
  once<T extends BaseEvent>(
    eventType: EventType,
    listener: EventListener<T>,
    options: {
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string;
    },
  ): () => void {
    const emitter = this.getEmitter(options.executionId);
    return emitter.once(eventType, listener, {
      filter: options.filter,
      timeout: options.timeout,
    });
  }

  /**
   * Cleanup all listeners and metrics associated with a specific execution
   * Should be called when execution completes (success, failure, or cancellation)
   *
   * @param executionId Execution ID
   * @returns Number of listeners cleaned up
   */
  cleanupExecutionListeners(executionId: string): number {
    if (!executionId) {
      logger.warn("cleanupExecutionListeners called with empty executionId");
      return 0;
    }

    let cleanedCount = 0;

    const emitter = this.emitters.get(executionId);
    if (emitter) {
      const listenerCounts = emitter.getListenerCount();
      for (const count of listenerCounts.values()) {
        cleanedCount += count;
      }

      emitter.removeAllListeners();
      this.emitters.delete(executionId);

      logger.debug("Cleaned up EventEmitter instance", {
        executionId,
        listenerCount: cleanedCount,
      });
    }

    this.metricsCollector.cleanupExecution(executionId);

    logger.info("Cleaned up execution-scoped listeners", {
      executionId,
      cleanedCount,
    });

    return cleanedCount;
  }

  /**
   * Get statistics for execution-scoped listeners
   *
   * @returns Map of execution ID to listener count
   */
  getExecutionListenerStats(): Map<string, number> {
    const stats = new Map<string, number>();

    for (const [executionId, emitter] of this.emitters.entries()) {
      const counts = emitter.getListenerCount();
      let total = 0;
      for (const count of counts.values()) {
        total += count;
      }
      stats.set(executionId, total);
    }

    return stats;
  }

  /**
   * Get metrics collector instance for cross-execution statistics
   *
   * @returns EventMetricsCollector instance
   */
  getMetricsCollector(): EventMetricsCollector {
    return this.metricsCollector;
  }

  /**
   * Get aggregated statistics for a specific event type
   *
   * @param eventType Event type to query
   * @returns Aggregated statistics or undefined if not found
   */
  getEventStatistics(eventType: string): AggregatedEventStat | undefined {
    return this.metricsCollector.getStatistics(eventType);
  }

  /**
   * Get complete metrics summary
   *
   * @returns Summary of all aggregated metrics
   */
  getMetricsSummary(): EventMetricsSummary {
    return this.metricsCollector.generateSummary();
  }
}

/**
 * Export EventRegistry class
 */
export { EventRegistry };
