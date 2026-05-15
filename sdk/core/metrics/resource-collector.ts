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
import type { MetricCollectorConfig } from "./types.js";
import { RESOURCE_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";

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
   * Export as Prometheus format
   */
  toPrometheus(): string[] {
    const summary = this.getResourceSummary();
    const metrics: PrometheusMetric[] = [];
    
    // Memory usage gauge
    metrics.push({
      name: 'resource_memory_usage_bytes',
      type: 'gauge',
      help: 'Memory usage in megabytes',
      samples: [{ value: summary.memoryUsageMB }]
    });
    
    // Active executions gauge
    metrics.push({
      name: 'resource_active_executions',
      type: 'gauge',
      help: 'Number of active executions',
      samples: [{ value: summary.activeExecutions }]
    });
    
    // Queued tasks gauge
    metrics.push({
      name: 'resource_queued_tasks',
      type: 'gauge',
      help: 'Number of queued tasks',
      samples: [{ value: summary.queuedTasks }]
    });
    
    // Event queue length gauge
    metrics.push({
      name: 'resource_event_queue_length',
      type: 'gauge',
      help: 'Event queue length',
      samples: [{ value: summary.eventQueueLength }]
    });
    
    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    return {
      type: 'resource',
      summary: this.getResourceSummary()
    };
  }
}
