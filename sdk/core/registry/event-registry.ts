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
import { RuntimeValidationError } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { EventMetricsCollector, type AggregatedEventStat, type EventMetricsSummary } from "../metrics/event-collector.js";
import { ExecutionEventEmitter } from "./event-emitter.js";

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
  
  // Configuration
  private config: Required<EventRegistryConfig>;
  
  // Cross-execution metrics collector (new universal metrics system)
  private metricsCollector: EventMetricsCollector;

  constructor(config?: EventRegistryConfig) {
    this.config = {
      maxListenersPerEvent: config?.maxListenersPerEvent ?? 100,
      defaultListenerTimeout: config?.defaultListenerTimeout ?? 30000, // 30 seconds
      slowListenerThreshold: config?.slowListenerThreshold ?? 5000, // 5 seconds
      enableBackpressure: config?.enableBackpressure ?? true,
    };
    
    // Initialize event metrics collector
    this.metricsCollector = new EventMetricsCollector();
  }

  /**
   * Get or create ExecutionEventEmitter for a specific execution
   * 
   * This is the preferred API for event management. Each execution gets its own
   * isolated emitter, eliminating the need to pass executionId parameters.
   * 
   * @param executionId Execution ID
   * @returns ExecutionEventEmitter instance for the execution
   * 
   * @example
   * ```typescript
   * // Get emitter for an execution
   * const emitter = eventRegistry.getEmitter(executionId);
   * 
   * // Register listener (no executionId parameter needed!)
   * emitter.on('NODE_COMPLETED', (event) => {
   *   console.log('Node completed:', event.nodeId);
   * });
   * 
   * // Emit event
   * await emitter.emit({ type: 'NODE_COMPLETED', nodeId: 'node-1' });
   * ```
   */
  getEmitter(executionId: string): ExecutionEventEmitter {
    if (!executionId) {
      throw new RuntimeValidationError("Execution ID is required", { field: "executionId" });
    }

    if (!this.emitters.has(executionId)) {
      logger.debug('Creating new ExecutionEventEmitter', { executionId });
      this.emitters.set(executionId, new ExecutionEventEmitter(executionId));
    }

    return this.emitters.get(executionId)!;
  }

  /**
   * Register event listener (delegates to EventEmitter)
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
      executionId: string; // Required execution ID
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
   * @param event Event object
   * @returns Promise that waits for all listeners to complete
   */
  async emit<T extends BaseEvent>(event: T): Promise<void> {
    if (!event.executionId) {
      throw new RuntimeValidationError("Event must have executionId", { field: "event.executionId" });
    }
    
    const emitter = this.getEmitter(event.executionId);
    await emitter.emit(event);
  }


  /**
   * Register one-time event listener
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
      executionId: string; // Required execution ID
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

    let cleanedCount = 0;

    // Cleanup EventEmitter instance
    const emitter = this.emitters.get(executionId);
    if (emitter) {
      // Get listener count before cleanup for reporting
      const listenerCounts = emitter.getListenerCount();
      for (const count of listenerCounts.values()) {
        cleanedCount += count;
      }
      
      // Remove all listeners from the emitter
      emitter.removeAllListeners();
      
      // Remove the emitter instance
      this.emitters.delete(executionId);
      
      logger.debug('Cleaned up EventEmitter instance', {
        executionId,
        listenerCount: cleanedCount,
      });
    }

    // Cleanup aggregated metrics for this execution
    this.metricsCollector.cleanupExecution(executionId);

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
   * @returns EventMetricsCollector instance
   * 
   * @example
   * ```typescript
   * // Get aggregated statistics
   * const collector = eventRegistry.getMetricsCollector();
   * const stats = collector.getStatistics('NODE_COMPLETED');
   * console.log(`Total nodes completed: ${stats?.count}`);
   * 
   * // Subscribe to periodic summaries
   * collector.onReport((report) => {
   *   console.log('Total events:', report.summary.totalMetrics);
   * }, { interval: 5000 });
   * ```
   */
  getMetricsCollector(): EventMetricsCollector {
    return this.metricsCollector;
  }

  /**
   * Get aggregated statistics for a specific event type
   * @param eventType Event type to query
   * @returns Aggregated statistics or undefined if not found
   */
  getEventStatistics(eventType: string): AggregatedEventStat | undefined {
    return this.metricsCollector.getStatistics(eventType);
  }

  /**
   * Get complete metrics summary
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
