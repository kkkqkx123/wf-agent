/**
 * In-Memory Metrics Storage Adapter
 * Fast, ephemeral storage for metrics data points, primarily for testing
 */

import type { MetricsStorageAdapter, MetricDataPoint, MetricsQuery } from "../types/adapter/metrics-storage-adapter.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("memory-metrics-storage");

export class MemoryMetricsStorage implements MetricsStorageAdapter {
  private metrics: MetricDataPoint[] = [];
  private initialized = false;

  async initialize(): Promise<void> {
    this.initialized = true;
    logger.info("MemoryMetricsStorage initialized");
  }

  async saveBatch(metrics: MetricDataPoint[]): Promise<void> {
    if (!this.initialized) {
      await this.initialize();
    }
    this.metrics.push(...metrics);
    logger.debug("Saved metrics batch", { count: metrics.length, total: this.metrics.length });
  }

  async query(query: MetricsQuery): Promise<MetricDataPoint[]> {
    if (!this.initialized) {
      await this.initialize();
    }

    let results = [...this.metrics];

    // Apply filters
    if (query.metricName) {
      results = results.filter(m => m.metricName === query.metricName);
    }
    if (query.metricType) {
      results = results.filter(m => m.metricType === query.metricType);
    }
    if (query.startTime !== undefined) {
      results = results.filter(m => m.timestamp >= query.startTime!);
    }
    if (query.endTime !== undefined) {
      results = results.filter(m => m.timestamp <= query.endTime!);
    }
    if (query.collectorName) {
      results = results.filter(m => m.collectorName === query.collectorName);
    }
    if (query.labels && Object.keys(query.labels).length > 0) {
      results = results.filter(m => {
        if (!m.labels) return false;
        return Object.entries(query.labels!).every(([k, v]) => m.labels![k] === v);
      });
    }

    // Sort
    const sortOrder = query.sortOrder === 'asc' ? 1 : -1;
    results.sort((a, b) => (a.timestamp - b.timestamp) * sortOrder);

    // Limit
    if (query.limit && query.limit > 0) {
      results = results.slice(0, query.limit);
    }

    return results;
  }

  async deleteOldMetrics(beforeTimestamp: number): Promise<number> {
    if (!this.initialized) {
      await this.initialize();
    }

    const before = this.metrics.length;
    this.metrics = this.metrics.filter(m => m.timestamp >= beforeTimestamp);
    const deleted = before - this.metrics.length;

    logger.info("Deleted old metrics", { count: deleted, beforeTimestamp });
    return deleted;
  }

  async close(): Promise<void> {
    this.metrics = [];
    this.initialized = false;
    logger.info("MemoryMetricsStorage closed");
  }
}
