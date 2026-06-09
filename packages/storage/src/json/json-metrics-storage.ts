/**
 * JSON File Metrics Storage Implementation
 * Stores metrics as a JSON array file on disk
 *
 * Design Note:
 * This storage directly implements MetricsStorageAdapter instead of extending
 * StorageAdapterBase because metrics have a different data model:
 * - Metrics are time-series data points (not single entities with metadata + blob)
 * - They use saveBatch() for bulk inserts instead of save()
 * - They use query() with time-range filters instead of list()
 * This design choice is intentional and consistent across all MetricsStorage implementations.
 */

import * as fs from "fs/promises";
import * as path from "path";
import { createModuleLogger } from "../logger.js";
import type { MetricsStorageAdapter, MetricDataPoint, MetricsQuery } from "../types/adapter/metrics-storage-adapter.js";

const logger = createModuleLogger("json-metrics-storage");

export interface JsonMetricsStorageConfig {
  /** Base directory for metrics data (default: cwd/data/metrics) */
  baseDir?: string;
  /** Metrics file name (default: metrics.json) */
  fileName?: string;
}

export class JsonMetricsStorage implements MetricsStorageAdapter {
  private metrics: MetricDataPoint[] = [];
  private metricsFilePath: string;
  private initialized = false;

  constructor(config: JsonMetricsStorageConfig = {}) {
    const baseDir = config.baseDir || path.join(process.cwd(), "data", "metrics");
    this.metricsFilePath = path.join(baseDir, config.fileName || "metrics.json");
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;

    try {
      // Ensure directory exists
      await fs.mkdir(path.dirname(this.metricsFilePath), { recursive: true });

      // Load existing metrics from file
      try {
        const content = await fs.readFile(this.metricsFilePath, "utf-8");
        this.metrics = JSON.parse(content) as MetricDataPoint[];
        logger.info("Loaded existing metrics", { count: this.metrics.length });
      } catch {
        // File doesn't exist yet, start fresh
        this.metrics = [];
        logger.info("Starting fresh metrics file");
      }

      this.initialized = true;
      logger.info("JsonMetricsStorage initialized", { filePath: this.metricsFilePath });
    } catch (error) {
      logger.error("Failed to initialize JsonMetricsStorage", { error });
      throw error;
    }
  }

  private async persist(): Promise<void> {
    try {
      await fs.writeFile(this.metricsFilePath, JSON.stringify(this.metrics), "utf-8");
    } catch (error) {
      logger.error("Failed to persist metrics", { error });
      throw error;
    }
  }

  async saveBatch(metrics: MetricDataPoint[]): Promise<void> {
    if (!this.initialized) {
      await this.initialize();
    }

    if (metrics.length === 0) return;

    this.metrics.push(...metrics);
    await this.persist();

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

    await this.persist();

    logger.info("Deleted old metrics", { count: deleted, beforeTimestamp });
    return deleted;
  }

  async close(): Promise<void> {
    if (this.metrics.length > 0) {
      await this.persist();
    }
    this.metrics = [];
    this.initialized = false;
    logger.info("JsonMetricsStorage closed");
  }
}
