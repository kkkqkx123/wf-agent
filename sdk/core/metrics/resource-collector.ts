/**
 * Resource Utilization Metrics Collector
 * 
 * Collects and aggregates metrics related to system resource utilization including:
 * - Memory usage
 * - Active execution count
 * - Queued task count
 * - Event queue length
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { RESOURCE_METRICS } from "./constants.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "ResourceMetricsCollector" });

/**
 * Resource-specific metric collector
 * Extends BaseMetricCollector with resource-specific convenience methods
 */
export class ResourceMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record memory usage
   * @param memoryUsageMB Memory usage in megabytes
   * @param component Optional component name (e.g., "executor", "registry")
   */
  recordMemoryUsage(memoryUsageMB: number, component?: string): void {
    const labels: Record<string, string> = {};
    
    if (component) {
      labels['component'] = component;
    }

    this.setGauge(RESOURCE_METRICS.MEMORY_USAGE, memoryUsageMB, labels);
  }

  /**
   * Record active execution count
   * @param count Number of active executions
   */
  recordActiveExecutions(count: number): void {
    this.setGauge(RESOURCE_METRICS.ACTIVE_EXECUTIONS, count);
  }

  /**
   * Record queued task count
   * @param count Number of queued tasks
   * @param queueType Optional queue type (e.g., "workflow", "tool", "llm")
   */
  recordQueuedTasks(count: number, queueType?: string): void {
    const labels: Record<string, string> = {};
    
    if (queueType) {
      labels['queue_type'] = queueType;
    }

    this.setGauge(RESOURCE_METRICS.QUEUED_TASKS, count, labels);
  }

  /**
   * Record event queue length
   * @param length Current event queue length
   */
  recordEventQueueLength(length: number): void {
    this.setGauge(RESOURCE_METRICS.EVENT_QUEUE_LENGTH, length);
  }

  /**
   * Record comprehensive resource snapshot
   * Captures all resource metrics at once for consistency
   * @param snapshot Resource snapshot data
   */
  recordResourceSnapshot(snapshot: {
    memoryUsageMB: number;
    activeExecutions: number;
    queuedTasks: number;
    eventQueueLength: number;
  }): void {
    this.recordMemoryUsage(snapshot.memoryUsageMB);
    this.recordActiveExecutions(snapshot.activeExecutions);
    this.recordQueuedTasks(snapshot.queuedTasks);
    this.recordEventQueueLength(snapshot.eventQueueLength);
  }

  /**
   * Get current resource utilization summary
   * @returns Latest values for all resource metrics
   */
  getResourceSummary(): {
    memoryUsageMB: number;
    activeExecutions: number;
    queuedTasks: number;
    eventQueueLength: number;
  } {
    const result = this.query({});
    
    let memoryUsageMB = 0;
    let activeExecutions = 0;
    let queuedTasks = 0;
    let eventQueueLength = 0;

    for (const [metricName, aggregated] of result.metrics.entries()) {
      // For gauges, take the latest value
      const value = aggregated.value;

      switch (metricName) {
        case RESOURCE_METRICS.MEMORY_USAGE:
          memoryUsageMB = value;
          break;
        case RESOURCE_METRICS.ACTIVE_EXECUTIONS:
          activeExecutions = value;
          break;
        case RESOURCE_METRICS.QUEUED_TASKS:
          queuedTasks = value;
          break;
        case RESOURCE_METRICS.EVENT_QUEUE_LENGTH:
          eventQueueLength = value;
          break;
      }
    }

    return {
      memoryUsageMB,
      activeExecutions,
      queuedTasks,
      eventQueueLength,
    };
  }

  /**
   * Flush metrics to storage
   * Override to implement custom persistence logic
   */
  async flush(): Promise<void> {
    const flushedCount = this.metricsBuffer.length;

    if (flushedCount > 0) {
      logger.debug("Flushing resource metrics", { flushedCount });

      // TODO: Implement actual persistence (e.g., write to database, send to monitoring service)
      // For now, just clear the buffer
      this.metricsBuffer = [];
    }
  }
}
