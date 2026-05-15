/**
 * Base Metric Collector Implementation
 * 
 * Provides common functionality for all metric collectors including:
 * - Buffering and batching
 * - Periodic flushing
 * - Query support
 * - Reporting
 * - Automatic expiration cleanup
 */

import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type {
  Metric,
  MetricCollector,
  MetricCollectorConfig,
  MetricFilter,
  MetricQueryResult,
  AggregatedMetric,
  MetricReportCallback,
  MetricReport,
} from "./types.js";

const logger = createContextualLogger({ operation: "BaseMetricCollector" });

// =============================================================================
// Internal State Types
// =============================================================================

/**
 * Histogram state for cumulative bucket counts
 */
interface HistogramState {
  buckets: Map<number, number>; // upperBound -> cumulative count
  sum: number;
  count: number;
  lastUpdate: number;
}

/**
 * Summary state for sliding window percentiles
 */
interface SummaryState {
  ringBuffer: Float64Array; // Fixed-size circular buffer
  bufferSize: number;
  writeIndex: number;
  filledCount: number;
  sum: number;
  count: number;
  lastUpdate: number;
}

/**
 * Internal self-monitoring metrics
 */
interface CollectorInternalMetrics {
  // Buffer state
  bufferSize: number;
  bufferUtilization: number;
  
  // Operation counts
  recordCount: number;
  flushCount: number;
  queryCount: number;
  
  // Performance metrics
  avgFlushDuration: number;
  avgQueryDuration: number;
  lastFlushDuration: number;
  
  // Cleanup statistics
  cleanupCount: number;
  expiredMetricsRemoved: number;
  lastCleanupTime: number;
  
  // Error counts
  flushErrorCount: number;
  reportErrorCount: number;
  
  // Subscription stats
  activeSubscriptions: number;
  
  // Memory estimate (bytes)
  estimatedMemoryUsage: number;
}

/**
 * Base implementation of MetricCollector
 * Provides common functionality that can be extended by specific collectors
 */
export abstract class BaseMetricCollector implements MetricCollector {
  protected metricsBuffer: Metric[] = [];
  protected config: Required<MetricCollectorConfig>;
  protected reportCallbacks: Array<{
    id: string;
    callback: MetricReportCallback;
    interval: number;
    timer?: NodeJS.Timeout;
  }> = [];
  private flushTimer?: NodeJS.Timeout;
  private cleanupTimer?: NodeJS.Timeout;
  private isFlushing: boolean = false;
  private nextSubscriptionId: number = 0;
  
  // Histogram state management for cumulative bucket counts
  private histogramStates: Map<string, HistogramState> = new Map();
  
  // Summary state management for sliding window percentiles
  private summaryStates: Map<string, SummaryState> = new Map();
  
  // Internal self-monitoring metrics
  private internalMetrics: CollectorInternalMetrics = {
    bufferSize: 0,
    bufferUtilization: 0,
    recordCount: 0,
    flushCount: 0,
    queryCount: 0,
    avgFlushDuration: 0,
    avgQueryDuration: 0,
    lastFlushDuration: 0,
    cleanupCount: 0,
    expiredMetricsRemoved: 0,
    lastCleanupTime: 0,
    flushErrorCount: 0,
    reportErrorCount: 0,
    activeSubscriptions: 0,
    estimatedMemoryUsage: 0,
  };
  
  // Standard histogram buckets (Prometheus default)
  private static readonly DEFAULT_HISTOGRAM_BUCKETS = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, Infinity
  ];
  
  // Default summary window size
  private static readonly DEFAULT_SUMMARY_WINDOW_SIZE = 1000;

  constructor(config?: MetricCollectorConfig) {
    this.config = {
      bufferSize: config?.bufferSize ?? 100,
      flushInterval: config?.flushInterval ?? 5000,
      enablePeriodicReporting: config?.enablePeriodicReporting ?? false,
      reportingInterval: config?.reportingInterval ?? 10000,
      maxAge: config?.maxAge ?? 3600000, // 1 hour default
    };

    // Start periodic flush if enabled
    if (this.config.flushInterval > 0) {
      this.startPeriodicFlush();
    }

    // Start periodic cleanup for expired metrics
    this.startPeriodicCleanup();
  }

  /**
   * Record a metric
   * @param metric The metric to record
   */
  record(metric: Metric): void {
    // Validate metric
    if (!metric.metricName) {
      logger.warn("Record called with missing metricName", { metric });
      return;
    }

    // Add timestamp if not present
    if (!metric.timestamp) {
      metric.timestamp = now();
    }

    // Add to buffer
    this.metricsBuffer.push(metric);
    
    // Update internal metrics
    this.internalMetrics.recordCount += 1;
    this.internalMetrics.bufferSize = this.metricsBuffer.length;
    this.internalMetrics.bufferUtilization = 
      this.metricsBuffer.length / this.config.bufferSize;
    this.updateMemoryEstimate();

    // Check if buffer needs flushing
    if (this.metricsBuffer.length >= this.config.bufferSize) {
      this.flush().catch((error) => {
        logger.error("Failed to flush metrics buffer", { error });
      });
    }
  }

  /**
   * Record a counter increment
   */
  incrementCounter(
    metricName: string,
    labels?: Record<string, string>,
    increment: number = 1,
  ): void {
    const metric = {
      metricName,
      metricType: "counter" as const,
      timestamp: now(),
      labels: labels || {},
      value: increment,
    };
    this.record(metric);
  }

  /**
   * Record a gauge value
   */
  setGauge(metricName: string, value: number, labels?: Record<string, string>): void {
    const metric = {
      metricName,
      metricType: "gauge" as const,
      timestamp: now(),
      labels: labels || {},
      value,
    };
    this.record(metric);
  }

  /**
   * Record a histogram observation with cumulative bucket counts
   */
  observeHistogram(
    metricName: string,
    value: number,
    labels?: Record<string, string>,
  ): void {
    const key = this.getHistogramKey(metricName, labels);
    let state = this.histogramStates.get(key);
    
    if (!state) {
      state = this.initializeHistogramState();
      this.histogramStates.set(key, state);
    }
    
    // Update cumulative buckets
    for (const bound of state.buckets.keys()) {
      if (value <= bound) {
        state.buckets.set(bound, state.buckets.get(bound)! + 1);
      }
    }
    
    state.sum += value;
    state.count += 1;
    state.lastUpdate = now();
    
    // Serialize buckets for the metric record
    const buckets = this.serializeBuckets(state.buckets);
    
    const metric = {
      metricName,
      metricType: "histogram" as const,
      timestamp: now(),
      labels: labels || {},
      value,
      buckets,
      sum: state.sum,
      count: state.count,
    };
    this.record(metric);
  }

  /**
   * Record a summary observation with sliding window percentiles
   */
  observeSummary(
    metricName: string,
    value: number,
    labels?: Record<string, string>,
  ): void {
    const key = this.getSummaryKey(metricName, labels);
    let state = this.summaryStates.get(key);
    
    if (!state) {
      state = this.initializeSummaryState();
      this.summaryStates.set(key, state);
    }
    
    // Write to ring buffer (circular)
    state.ringBuffer[state.writeIndex] = value;
    state.writeIndex = (state.writeIndex + 1) % state.bufferSize;
    state.filledCount = Math.min(state.filledCount + 1, state.bufferSize);
    
    state.sum += value;
    state.count += 1;
    state.lastUpdate = now();
    
    // Calculate percentiles from current window
    const percentiles = this.calculatePercentiles(state);
    
    const metric = {
      metricName,
      metricType: "summary" as const,
      timestamp: now(),
      labels: labels || {},
      value,
      percentiles,
      sum: state.sum,
      count: state.count,
    };
    this.record(metric);
  }

  /**
   * Flush buffered metrics
   * Must be implemented by subclasses to handle persistence
   */
  abstract flush(): Promise<void>;

  /**
   * Query metrics with filters
   * Default implementation performs in-memory filtering
   * Subclasses can override for optimized querying
   */
  query(filter: MetricFilter): MetricQueryResult {
    const startTime = now();

    // Filter metrics
    let filtered = this.metricsBuffer;

    if (filter.metricName) {
      filtered = filtered.filter((m) => m.metricName === filter.metricName);
    }

    if (filter.metricType) {
      filtered = filtered.filter((m) => m.metricType === filter.metricType);
    }

    if (filter.labels) {
      filtered = filtered.filter((m) => {
        return Object.entries(filter.labels!).every(
          ([key, val]) => m.labels[key] === val,
        );
      });
    }

    if (filter.timeRange) {
      filtered = filtered.filter(
        (m) =>
          m.timestamp >= filter.timeRange!.from &&
          m.timestamp <= filter.timeRange!.to,
      );
    }

    // Apply limit
    if (filter.limit) {
      filtered = filtered.slice(0, filter.limit);
    }

    // Aggregate results
    const aggregated = this.aggregateMetrics(filtered);

    const queryTime = now() - startTime;
    
    // Update internal metrics
    this.internalMetrics.queryCount += 1;
    this.updateInternalAverage('avgQueryDuration', queryTime);

    return {
      totalCount: filtered.length,
      metrics: aggregated,
      queryTime,
    };
  }

  /**
   * Subscribe to periodic reports
   */
  onReport(
    callback: MetricReportCallback,
    options?: { interval?: number },
  ): () => void {
    const interval = options?.interval ?? this.config.reportingInterval;
    const subscriptionId = `sub_${++this.nextSubscriptionId}`;

    const subscription = {
      id: subscriptionId,
      callback,
      interval,
    };

    this.reportCallbacks.push(subscription);
    this.internalMetrics.activeSubscriptions = this.reportCallbacks.length;

    // Start periodic reporting if this is the first subscription
    if (this.reportCallbacks.length === 1) {
      this.startPeriodicReporting();
    }

    // Return unsubscribe function
    return () => {
      const index = this.reportCallbacks.findIndex(
        (sub) => sub.id === subscriptionId,
      );
      if (index !== -1) {
        const sub = this.reportCallbacks[index];
        if (sub?.timer) {
          clearInterval(sub.timer);
        }
        this.reportCallbacks.splice(index, 1);
        this.internalMetrics.activeSubscriptions = this.reportCallbacks.length;
        
        // Stop periodic reporting if no more subscriptions
        if (this.reportCallbacks.length === 0) {
          this.stopPeriodicReporting();
        }
      }
    };
  }

  /**
   * Clear all collected metrics
   */
  clear(): void {
    this.metricsBuffer = [];
    this.histogramStates.clear();
    this.summaryStates.clear();
    this.stopPeriodicFlush();
    this.stopPeriodicCleanup();
    this.stopPeriodicReporting();
    
    // Reset internal metrics (keep counts, reset current state)
    this.internalMetrics.bufferSize = 0;
    this.internalMetrics.bufferUtilization = 0;
    this.internalMetrics.estimatedMemoryUsage = 0;
    
    logger.info("Metrics cleared");
  }

  /**
   * Dispose the collector and release resources
   */
  dispose(): void {
    // Flush remaining metrics before disposal
    this.flush().catch((error) => {
      logger.error("Failed to flush metrics during disposal", { error });
    });
    
    this.stopPeriodicFlush();
    this.stopPeriodicCleanup();
    this.stopPeriodicReporting();
    this.clear();
    logger.info("Metric collector disposed");
  }

  /**
   * Export metrics in Prometheus exposition format
   * Must be implemented by subclasses to provide specific metric formatting
   */
  abstract toPrometheus(): string[];

  /**
   * Export metrics as JSON
   * Must be implemented by subclasses to provide specific JSON structure
   */
  abstract toJSON(): Record<string, unknown>;

  /**
   * Get internal monitoring metrics
   * @returns Current internal metrics snapshot
   */
  getInternalMetrics(): CollectorInternalMetrics {
    return { ...this.internalMetrics };
  }
  
  /**
   * Export internal metrics in Prometheus format
   * @returns Array of formatted metric lines
   */
  exportInternalMetrics(): string[] {
    const metrics: string[] = [];
    const m = this.internalMetrics;
    const collectorName = this.constructor.name;
    
    // Buffer size
    metrics.push(`# HELP metrics_buffer_size Current buffer size`);
    metrics.push(`# TYPE metrics_buffer_size gauge`);
    metrics.push(`metrics_buffer_size{collector="${collectorName}"} ${m.bufferSize}`);
    
    // Buffer utilization
    metrics.push(`# HELP metrics_buffer_utilization Buffer utilization ratio (0-1)`);
    metrics.push(`# TYPE metrics_buffer_utilization gauge`);
    metrics.push(`metrics_buffer_utilization{collector="${collectorName}"} ${m.bufferUtilization.toFixed(3)}`);
    
    // Record count
    metrics.push(`# HELP metrics_record_total Total records`);
    metrics.push(`# TYPE metrics_record_total counter`);
    metrics.push(`metrics_record_total{collector="${collectorName}"} ${m.recordCount}`);
    
    // Flush statistics
    metrics.push(`# HELP metrics_flush_total Total flush operations`);
    metrics.push(`# TYPE metrics_flush_total counter`);
    metrics.push(`metrics_flush_total{collector="${collectorName}"} ${m.flushCount}`);
    
    metrics.push(`# HELP metrics_flush_error_total Total flush errors`);
    metrics.push(`# TYPE metrics_flush_error_total counter`);
    metrics.push(`metrics_flush_error_total{collector="${collectorName}"} ${m.flushErrorCount}`);
    
    metrics.push(`# HELP metrics_flush_duration_seconds Average flush duration`);
    metrics.push(`# TYPE metrics_flush_duration_seconds gauge`);
    metrics.push(`metrics_flush_duration_seconds{collector="${collectorName}"} ${(m.avgFlushDuration / 1000).toFixed(3)}`);
    
    // Query statistics
    metrics.push(`# HELP metrics_query_total Total query operations`);
    metrics.push(`# TYPE metrics_query_total counter`);
    metrics.push(`metrics_query_total{collector="${collectorName}"} ${m.queryCount}`);
    
    metrics.push(`# HELP metrics_query_duration_seconds Average query duration`);
    metrics.push(`# TYPE metrics_query_duration_seconds gauge`);
    metrics.push(`metrics_query_duration_seconds{collector="${collectorName}"} ${(m.avgQueryDuration / 1000).toFixed(3)}`);
    
    // Cleanup statistics
    metrics.push(`# HELP metrics_cleanup_total Total cleanup operations`);
    metrics.push(`# TYPE metrics_cleanup_total counter`);
    metrics.push(`metrics_cleanup_total{collector="${collectorName}"} ${m.cleanupCount}`);
    
    metrics.push(`# HELP metrics_cleanup_removed_total Total expired metrics removed`);
    metrics.push(`# TYPE metrics_cleanup_removed_total counter`);
    metrics.push(`metrics_cleanup_removed_total{collector="${collectorName}"} ${m.expiredMetricsRemoved}`);
    
    // Subscriptions
    metrics.push(`# HELP metrics_active_subscriptions Active report subscriptions`);
    metrics.push(`# TYPE metrics_active_subscriptions gauge`);
    metrics.push(`metrics_active_subscriptions{collector="${collectorName}"} ${m.activeSubscriptions}`);
    
    // Memory estimate
    metrics.push(`# HELP metrics_memory_estimate_bytes Estimated memory usage`);
    metrics.push(`# TYPE metrics_memory_estimate_bytes gauge`);
    metrics.push(`metrics_memory_estimate_bytes{collector="${collectorName}"} ${m.estimatedMemoryUsage}`);
    
    return metrics;
  }

  // =============================================================================
  // Protected Helper Methods
  // =============================================================================

  /**
   * Get histogram state key
   */
  protected getHistogramKey(metricName: string, labels?: Record<string, string>): string {
    return `${metricName}:${JSON.stringify(labels || {})}`;
  }
  
  /**
   * Initialize histogram state with default buckets
   */
  protected initializeHistogramState(): HistogramState {
    const buckets = new Map<number, number>();
    for (const bound of BaseMetricCollector.DEFAULT_HISTOGRAM_BUCKETS) {
      buckets.set(bound, 0);
    }
    return { buckets, sum: 0, count: 0, lastUpdate: 0 };
  }
  
  /**
   * Serialize histogram buckets to array
   */
  protected serializeBuckets(buckets: Map<number, number>): Array<{ upperBound: number; count: number }> {
    return Array.from(buckets.entries())
      .map(([upperBound, count]) => ({ upperBound, count }))
      .sort((a, b) => a.upperBound - b.upperBound);
  }
  
  /**
   * Get summary state key
   */
  protected getSummaryKey(metricName: string, labels?: Record<string, string>): string {
    return `${metricName}:${JSON.stringify(labels || {})}`;
  }
  
  /**
   * Initialize summary state with sliding window
   */
  protected initializeSummaryState(windowSize?: number): SummaryState {
    const size = windowSize || BaseMetricCollector.DEFAULT_SUMMARY_WINDOW_SIZE;
    return {
      ringBuffer: new Float64Array(size),
      bufferSize: size,
      writeIndex: 0,
      filledCount: 0,
      sum: 0,
      count: 0,
      lastUpdate: 0,
    };
  }
  
  /**
   * Calculate percentiles from summary state
   */
  protected calculatePercentiles(
    state: SummaryState,
    targets: number[] = [0.5, 0.9, 0.95, 0.99]
  ): Array<{ percentile: number; value: number }> {
    if (state.filledCount === 0) {
      return targets.map(p => ({ percentile: p, value: 0 }));
    }
    
    // Extract valid data from ring buffer
    const values = new Float64Array(state.filledCount);
    for (let i = 0; i < state.filledCount; i++) {
      const idx = (state.writeIndex + i) % state.bufferSize;
      const val = state.ringBuffer[idx];
      if (val !== undefined) {
        values[i] = val;
      }
    }
    
    // Sort for percentile calculation
    values.sort();
    
    // Calculate percentiles
    return targets.map(p => ({
      percentile: p,
      value: this.getPercentileValue(values, p),
    }));
  }
  
  /**
   * Get value at specific percentile from sorted array
   */
  protected getPercentileValue(sortedValues: Float64Array, percentile: number): number {
    if (sortedValues.length === 0) return 0;
    
    const index = percentile * (sortedValues.length - 1);
    const lower = Math.floor(index);
    const upper = Math.ceil(index);
    
    if (lower === upper || upper >= sortedValues.length) {
      return sortedValues[lower] ?? 0;
    }
    
    // Linear interpolation
    const weight = index - lower;
    const lowerVal = sortedValues[lower] ?? 0;
    const upperVal = sortedValues[upper] ?? 0;
    return lowerVal * (1 - weight) + upperVal * weight;
  }
  
  /**
   * Update internal metrics averages
   */
  protected updateInternalAverage(field: keyof CollectorInternalMetrics, value: number): void {
    const currentAvg = this.internalMetrics[field] as number;
    const count = field === 'avgFlushDuration' 
      ? this.internalMetrics.flushCount 
      : this.internalMetrics.queryCount;
    
    if (count > 0) {
      this.internalMetrics[field] = currentAvg + (value - currentAvg) / count;
    }
  }
  
  /**
   * Update memory usage estimate
   */
  protected updateMemoryEstimate(): void {
    // Rough estimate: ~500 bytes per metric
    this.internalMetrics.estimatedMemoryUsage = this.metricsBuffer.length * 500;
  }

  /**
   * Aggregate metrics by name and labels
   */
  protected aggregateMetrics(metrics: Metric[]): Map<string, AggregatedMetric> {
    const aggregated = new Map<string, AggregatedMetric>();

    for (const metric of metrics) {
      if (!aggregated.has(metric.metricName)) {
        aggregated.set(metric.metricName, {
          metricName: metric.metricName,
          metricType: metric.metricType,
          value: 0,
          byLabel: new Map(),
          timeSeries: [],
        });
      }

      const agg = aggregated.get(metric.metricName)!;
      const numericValue = typeof metric.value === "number" ? metric.value : 0;

      // Update aggregated value based on metric type
      switch (metric.metricType) {
        case "counter":
          // Counters accumulate
          agg.value += numericValue;
          break;
        case "gauge":
          // Gauges take the latest value
          agg.value = numericValue;
          break;
        case "histogram":
          // Histograms accumulate sum and count
          agg.value = numericValue; // Keep last observed value
          break;
        case "summary":
          // Summaries accumulate sum and count
          agg.value = numericValue; // Keep last observed value
          break;
      }

      // Add to time series
      if (typeof metric.value === "number") {
        agg.timeSeries!.push({
          timestamp: metric.timestamp,
          value: metric.value,
        });
      }

      // Group by labels
      const labelKey = JSON.stringify(metric.labels);
      if (!agg.byLabel.has(labelKey)) {
        agg.byLabel.set(labelKey, {
          metricName: metric.metricName,
          metricType: metric.metricType,
          value: 0,
          byLabel: new Map(),
        });
      }

      const labelAgg = agg.byLabel.get(labelKey)!;
      // Apply same aggregation logic for label groups
      switch (metric.metricType) {
        case "counter":
          labelAgg.value += numericValue;
          break;
        case "gauge":
          labelAgg.value = numericValue;
          break;
        case "histogram":
        case "summary":
          labelAgg.value = numericValue;
          break;
      }
    }

    return aggregated;
  }

  /**
   * Create standard histogram buckets
   */
  protected createStandardBuckets(value: number): Array<{
    upperBound: number;
    count: number;
  }> {
    const standardBounds = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10];
    const buckets: Array<{ upperBound: number; count: number }> = [];

    for (const bound of standardBounds) {
      buckets.push({
        upperBound: bound,
        count: value <= bound ? 1 : 0,
      });
    }

    // Add +Inf bucket
    buckets.push({
      upperBound: Infinity,
      count: 1,
    });

    return buckets;
  }

  /**
   * Generate a metric report
   */
  protected generateReport(): MetricReport {
    const byType: Record<string, number> = {
      counter: 0,
      gauge: 0,
      histogram: 0,
      summary: 0,
    };

    const byCategory: Record<string, number> = {};

    for (const metric of this.metricsBuffer) {
      byType[metric.metricType] = (byType[metric.metricType] || 0) + 1;

      // Extract category from metric name (first part before '.')
      const category = metric.metricName.split(".")[0] || "unknown";
      byCategory[category] = (byCategory[category] ?? 0) + 1;
    }

    // Get top metrics by value
    const sortedMetrics = [...this.metricsBuffer]
      .filter((m) => typeof m.value === "number")
      .sort((a, b) => (b.value as number) - (a.value as number))
      .slice(0, 10)
      .map((m) => ({
        metricName: m.metricName,
        value: m.value as number,
        labels: m.labels,
      }));

    return {
      timestamp: now(),
      summary: {
        totalMetrics: this.metricsBuffer.length,
        byType,
        byCategory,
      },
      topMetrics: sortedMetrics,
    };
  }

  /**
   * Start periodic flushing
   */
  private startPeriodicFlush(): void {
    if (this.flushTimer) {
      return;
    }

    this.flushTimer = setInterval(() => {
      // Prevent concurrent flushes
      if (!this.isFlushing) {
        this.isFlushing = true;
        const startTime = now();
        this.flush()
          .then(() => {
            const duration = now() - startTime;
            this.internalMetrics.flushCount += 1;
            this.internalMetrics.lastFlushDuration = duration;
            this.updateInternalAverage('avgFlushDuration', duration);
          })
          .catch((error) => {
            logger.error("Periodic flush failed", { error });
            this.internalMetrics.flushErrorCount += 1;
          })
          .finally(() => {
            this.isFlushing = false;
          });
      }
    }, this.config.flushInterval);

    logger.debug("Periodic flush started", { interval: this.config.flushInterval });
  }

  /**
   * Stop periodic flushing
   */
  private stopPeriodicFlush(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = undefined;
      logger.debug("Periodic flush stopped");
    }
  }

  /**
   * Start periodic cleanup of expired metrics
   */
  private startPeriodicCleanup(): void {
    if (this.cleanupTimer) {
      return;
    }

    // Run cleanup every minute or maxAge/10, whichever is smaller
    const cleanupInterval = Math.min(60000, this.config.maxAge / 10);
    
    this.cleanupTimer = setInterval(() => {
      this.cleanupExpiredMetrics();
    }, cleanupInterval);

    logger.debug("Periodic cleanup started", { 
      interval: cleanupInterval,
      maxAge: this.config.maxAge 
    });
  }

  /**
   * Stop periodic cleanup
   */
  private stopPeriodicCleanup(): void {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = undefined;
      logger.debug("Periodic cleanup stopped");
    }
  }

  /**
   * Remove expired metrics from buffer
   */
  private cleanupExpiredMetrics(): void {
    const cutoffTime = now() - this.config.maxAge;
    const originalCount = this.metricsBuffer.length;
    
    this.metricsBuffer = this.metricsBuffer.filter(
      (metric) => metric.timestamp >= cutoffTime
    );
    
    const removedCount = originalCount - this.metricsBuffer.length;
    if (removedCount > 0) {
      this.internalMetrics.cleanupCount += 1;
      this.internalMetrics.expiredMetricsRemoved += removedCount;
      this.internalMetrics.lastCleanupTime = now();
      
      logger.debug("Cleaned up expired metrics", { 
        removedCount, 
        remainingCount: this.metricsBuffer.length 
      });
    }
  }

  /**
   * Start periodic reporting
   */
  private startPeriodicReporting(): void {
    for (const subscription of this.reportCallbacks) {
      if (subscription.timer) {
        // Already started
        continue;
      }
      
      subscription.timer = setInterval(async () => {
        try {
          const report = this.generateReport();
          await subscription.callback(report);
        } catch (error) {
          logger.error("Error in report callback", { 
            error,
            subscriptionId: subscription.id 
          });
        }
      }, subscription.interval);
    }

    logger.info("Periodic reporting started", {
      subscriptionCount: this.reportCallbacks.length,
    });
  }

  /**
   * Stop periodic reporting
   */
  private stopPeriodicReporting(): void {
    for (const subscription of this.reportCallbacks) {
      if (subscription.timer) {
        clearInterval(subscription.timer);
        subscription.timer = undefined;
      }
    }

    logger.info("Periodic reporting stopped");
  }
}
