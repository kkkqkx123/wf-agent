/**
 * MetricsAggregator - Cross-Execution Metrics Aggregation
 * 
 * Collects and aggregates event metrics across all workflow executions.
 * Provides statistical insights without requiring global event listeners.
 * 
 * Design Principles:
 * - Separate real-time events (per-execution) from aggregated statistics (cross-execution)
 * - Use metrics collection instead of global listeners for cross-execution monitoring
 * - Provide periodic summary updates for dashboards and monitoring
 * - No memory leak risk (aggregated numbers, not function references)
 */

import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "MetricsAggregator" });

/**
 * Single metric record from an execution
 */
export interface ExecutionMetric {
  executionId: string;
  eventType: string;
  timestamp: number;
  metadata?: Record<string, unknown>;
}

/**
 * Aggregated statistics for a specific event type
 */
export interface AggregatedStat {
  /** Total count of this event type across all executions */
  count: number;
  /** Timestamp of the most recent occurrence */
  lastSeen: number;
  /** Count breakdown by execution ID */
  byExecution: Map<string, number>;
  /** First seen timestamp */
  firstSeen?: number;
}

/**
 * Summary statistics across all event types
 */
export interface MetricsSummary {
  /** Total events recorded */
  totalEvents: number;
  /** Statistics by event type */
  byEventType: Map<string, AggregatedStat>;
  /** Active execution count */
  activeExecutions: number;
  /** Summary generation timestamp */
  generatedAt: number;
}

/**
 * Configuration for MetricsAggregator
 */
export interface MetricsAggregatorConfig {
  /** Buffer size before automatic flush (default: 1000) */
  bufferSize?: number;
  /** Enable automatic periodic summaries (default: false) */
  enablePeriodicSummaries?: boolean;
  /** Summary interval in milliseconds (default: 5000) */
  summaryInterval?: number;
}

/**
 * Callback function for summary notifications
 */
export type SummaryCallback = (summary: MetricsSummary) => void | Promise<void>;

/**
 * MetricsAggregator - Central metrics collection and aggregation
 * 
 * Responsibilities:
 * - Collect metrics from all executions
 * - Aggregate statistics by event type
 * - Track per-execution breakdowns
 * - Provide periodic summary updates
 * - Support querying historical statistics
 */
export class MetricsAggregator {
  private metricsBuffer: ExecutionMetric[] = [];
  private aggregatedStats: Map<string, AggregatedStat> = new Map();
  private summaryCallbacks: Array<{ callback: SummaryCallback; interval: number; timer?: NodeJS.Timeout }> = [];
  private config: Required<MetricsAggregatorConfig>;
  private isStarted: boolean = false;

  constructor(config?: MetricsAggregatorConfig) {
    this.config = {
      bufferSize: config?.bufferSize ?? 1000,
      enablePeriodicSummaries: config?.enablePeriodicSummaries ?? false,
      summaryInterval: config?.summaryInterval ?? 5000,
    };
  }

  /**
   * Record a metric from an execution
   * @param metric The metric to record
   */
  record(metric: ExecutionMetric): void {
    // Validate input
    if (!metric.executionId) {
      logger.warn("Record called with missing executionId", { metric });
      return;
    }
    if (!metric.eventType) {
      logger.warn("Record called with missing eventType", { metric });
      return;
    }

    // Add to buffer
    this.metricsBuffer.push(metric);

    // Update aggregated statistics
    this.updateAggregatedStats(metric);

    // Check if buffer needs flushing
    if (this.metricsBuffer.length >= this.config.bufferSize) {
      this.flush();
    }
  }

  /**
   * Update aggregated statistics for a metric
   * @param metric The metric to aggregate
   */
  private updateAggregatedStats(metric: ExecutionMetric): void {
    const { eventType, executionId, timestamp } = metric;

    // Get or create stat for this event type
    if (!this.aggregatedStats.has(eventType)) {
      this.aggregatedStats.set(eventType, {
        count: 0,
        lastSeen: 0,
        byExecution: new Map(),
        firstSeen: timestamp,
      });
    }

    const stat = this.aggregatedStats.get(eventType)!;
    
    // Update counters
    stat.count++;
    stat.lastSeen = Math.max(stat.lastSeen, timestamp);
    
    // Update per-execution breakdown
    const currentCount = stat.byExecution.get(executionId) || 0;
    stat.byExecution.set(executionId, currentCount + 1);
  }

  /**
   * Get aggregated statistics for a specific event type
   * @param eventType Event type to query
   * @returns Aggregated statistics or undefined if not found
   */
  getStatistics(eventType: string): AggregatedStat | undefined {
    return this.aggregatedStats.get(eventType);
  }

  /**
   * Get all aggregated statistics
   * @returns Map of event type to statistics
   */
  getAllStatistics(): Map<string, AggregatedStat> {
    return new Map(this.aggregatedStats);
  }

  /**
   * Generate a complete metrics summary
   * @returns Comprehensive summary of all metrics
   */
  generateSummary(): MetricsSummary {
    // Calculate total events
    let totalEvents = 0;
    for (const stat of this.aggregatedStats.values()) {
      totalEvents += stat.count;
    }

    // Count active executions (executions with at least one event)
    const activeExecutions = new Set<string>();
    for (const stat of this.aggregatedStats.values()) {
      for (const executionId of stat.byExecution.keys()) {
        activeExecutions.add(executionId);
      }
    }

    return {
      totalEvents,
      byEventType: new Map(this.aggregatedStats),
      activeExecutions: activeExecutions.size,
      generatedAt: now(),
    };
  }

  /**
   * Subscribe to periodic summary updates
   * @param callback Function to call with summary data
   * @param options Subscription options
   * @returns Unsubscribe function
   */
  onSummary(callback: SummaryCallback, options?: { interval?: number }): () => void {
    const interval = options?.interval ?? this.config.summaryInterval;
    
    const subscription: { callback: SummaryCallback; interval: number; timer?: NodeJS.Timeout } = {
      callback,
      interval,
    };

    this.summaryCallbacks.push(subscription);

    // Start timer if periodic summaries are enabled
    if (this.config.enablePeriodicSummaries && !this.isStarted) {
      this.startPeriodicSummaries();
    }

    // Return unsubscribe function
    return () => {
      const index = this.summaryCallbacks.indexOf(subscription);
      if (index !== -1) {
        if (subscription.timer) {
          clearInterval(subscription.timer);
        }
        this.summaryCallbacks.splice(index, 1);
      }
    };
  }

  /**
   * Start periodic summary emission
   */
  private startPeriodicSummaries(): void {
    if (this.isStarted) {
      return;
    }

    this.isStarted = true;

    // Set up timers for each subscription
    for (const subscription of this.summaryCallbacks) {
      subscription.timer = setInterval(async () => {
        try {
          const summary = this.generateSummary();
          await subscription.callback(summary);
        } catch (error) {
          logger.error("Error in summary callback", { error });
        }
      }, subscription.interval);
    }

    logger.info("Periodic summaries started", {
      subscriptionCount: this.summaryCallbacks.length,
    });
  }

  /**
   * Stop periodic summary emission
   */
  stopPeriodicSummaries(): void {
    if (!this.isStarted) {
      return;
    }

    // Clear all timers
    for (const subscription of this.summaryCallbacks) {
      if (subscription.timer) {
        clearInterval(subscription.timer);
        subscription.timer = undefined;
      }
    }

    this.isStarted = false;
    logger.info("Periodic summaries stopped");
  }

  /**
   * Flush the metrics buffer
   * Currently just clears the buffer, but could be extended to persist to storage
   */
  flush(): void {
    const flushedCount = this.metricsBuffer.length;
    
    if (flushedCount > 0) {
      logger.debug("Flushing metrics buffer", { flushedCount });
      this.metricsBuffer = [];
    }
  }

  /**
   * Clear all aggregated statistics
   * Useful for testing or resetting metrics
   */
  clear(): void {
    this.metricsBuffer = [];
    this.aggregatedStats.clear();
    this.stopPeriodicSummaries();
    this.summaryCallbacks = [];
    logger.info("Metrics aggregator cleared");
  }

  /**
   * Get current buffer size
   * @returns Number of metrics waiting to be flushed
   */
  getBufferSize(): number {
    return this.metricsBuffer.length;
  }

  /**
   * Get total number of recorded events
   * @returns Total count across all event types
   */
  getTotalEventCount(): number {
    let total = 0;
    for (const stat of this.aggregatedStats.values()) {
      total += stat.count;
    }
    return total;
  }

  /**
   * Get list of active execution IDs
   * @returns Set of execution IDs that have recorded events
   */
  getActiveExecutions(): Set<string> {
    const executions = new Set<string>();
    for (const stat of this.aggregatedStats.values()) {
      for (const executionId of stat.byExecution.keys()) {
        executions.add(executionId);
      }
    }
    return executions;
  }

  /**
   * Cleanup metrics for a specific execution
   * Called when an execution completes to free memory
   * @param executionId Execution ID to cleanup
   * @returns Number of statistics entries cleaned
   */
  cleanupExecution(executionId: string): number {
    let cleanedCount = 0;

    for (const [eventType, stat] of this.aggregatedStats.entries()) {
      if (stat.byExecution.delete(executionId)) {
        cleanedCount++;
        
        // If no more executions for this event type, remove it
        if (stat.byExecution.size === 0) {
          this.aggregatedStats.delete(eventType);
        }
      }
    }

    if (cleanedCount > 0) {
      logger.debug("Cleaned up execution metrics", { executionId, cleanedCount });
    }

    return cleanedCount;
  }

  /**
   * Dispose the aggregator and release all resources
   */
  dispose(): void {
    this.stopPeriodicSummaries();
    this.clear();
    logger.info("Metrics aggregator disposed");
  }
}
