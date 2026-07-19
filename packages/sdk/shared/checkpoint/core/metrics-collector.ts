import type {
  CheckpointMetricsConfig,
  CheckpointCreationMetrics,
  CheckpointLoadMetrics,
  CheckpointCleanupMetrics,
  CheckpointChainLengthMetric,
  CheckpointMetricsEvent,
} from "@wf-agent/types";
import type {
  MetricCollector,
  Metric,
  MetricFilter,
  MetricQueryResult,
  MetricReportCallback,
  AggregatedMetric,
} from "@wf-agent/common-utils";

interface Logger {
  debug(msg: string, context?: Record<string, unknown>): void;
  info(msg: string, context?: Record<string, unknown>): void;
  warn(msg: string, context?: Record<string, unknown>): void;
  error(msg: string, context?: Record<string, unknown>): void;
}

interface EventEmitter {
  (eventType: string, payload: unknown): Promise<void>;
}

export class CheckpointMetricsCollector implements MetricCollector {
  private logger: Logger;
  private metrics = new Map<string, any>();
  private runningAverages: {
    creationDuration: number[];
    loadDuration: number[];
    cleanupDuration: number[];
    creationSize: number[];
  } = {
    creationDuration: [],
    loadDuration: [],
    cleanupDuration: [],
    creationSize: [],
  };
  private eventEmitter?: EventEmitter;
  private listeners: Array<(event: CheckpointMetricsEvent) => void> = [];
  private static readonly MAX_RECENT_SAMPLES = 100;

  constructor(_config: CheckpointMetricsConfig, logger: Logger, eventEmitter?: EventEmitter) {
    this.logger = logger;
    this.eventEmitter = eventEmitter;
  }

  on(listener: (event: CheckpointMetricsEvent) => void): void {
    this.listeners.push(listener);
  }

  recordCreation(metrics: CheckpointCreationMetrics): void {
    const entityKey = `${metrics.entityId}:creation`;
    const existing = this.metrics.get(entityKey) ?? {};
    const updated = {
      ...existing,
      lastCreationDuration: metrics.duration,
      lastCreationSize: metrics.size,
      lastCreationTimestamp: metrics.timestamp,
      totalCreations: (existing.totalCreations ?? 0) + 1,
      successfulCreations: (existing.successfulCreations ?? 0) + (metrics.success ? 1 : 0),
      failedCreations: (existing.failedCreations ?? 0) + (metrics.success ? 0 : 1),
      creationDurations: [
        ...(existing.creationDurations ?? []).slice(-CheckpointMetricsCollector.MAX_RECENT_SAMPLES),
        metrics.duration,
      ],
    };
    this.metrics.set(entityKey, updated);

    this.runningAverages.creationDuration.push(metrics.duration);
    if (this.runningAverages.creationDuration.length > CheckpointMetricsCollector.MAX_RECENT_SAMPLES) {
      this.runningAverages.creationDuration.shift();
    }

    this.runningAverages.creationSize.push(metrics.size);
    if (this.runningAverages.creationSize.length > CheckpointMetricsCollector.MAX_RECENT_SAMPLES) {
      this.runningAverages.creationSize.shift();
    }

    this.emitEvent("creation", metrics);
  }

  recordLoad(metrics: CheckpointLoadMetrics): void {
    const entityKey = `${metrics.entityId}:load`;
    const existing = this.metrics.get(entityKey) ?? {};
    const updated = {
      ...existing,
      lastLoadDuration: metrics.duration,
      lastLoadTimestamp: metrics.timestamp,
      totalLoads: (existing.totalLoads ?? 0) + 1,
      successfulLoads: (existing.successfulLoads ?? 0) + (metrics.success ? 1 : 0),
      failedLoads: (existing.failedLoads ?? 0) + (metrics.success ? 0 : 1),
      loadDurations: [
        ...(existing.loadDurations ?? []).slice(-CheckpointMetricsCollector.MAX_RECENT_SAMPLES),
        metrics.duration,
      ],
    };
    this.metrics.set(entityKey, updated);

    this.runningAverages.loadDuration.push(metrics.duration);
    if (this.runningAverages.loadDuration.length > CheckpointMetricsCollector.MAX_RECENT_SAMPLES) {
      this.runningAverages.loadDuration.shift();
    }

    this.emitEvent("load", metrics);
  }

  recordCleanup(metrics: CheckpointCleanupMetrics): void {
    const entityKey = `${metrics.entityId}:cleanup`;
    const existing = this.metrics.get(entityKey) ?? {};
    const updated = {
      ...existing,
      lastCleanupDuration: metrics.duration,
      lastCleanupTimestamp: metrics.timestamp,
      totalCleanups: (existing.totalCleanups ?? 0) + 1,
      successfulCleanups: (existing.successfulCleanups ?? 0) + (metrics.success ? 1 : 0),
      failedCleanups: (existing.failedCleanups ?? 0) + (metrics.success ? 0 : 1),
      totalCheckpointsCleaned: (existing.totalCheckpointsCleaned ?? 0) + metrics.count,
      totalSpaceFreed: (existing.totalSpaceFreed ?? 0) + metrics.sizeFreed,
    };
    this.metrics.set(entityKey, updated);

    this.runningAverages.cleanupDuration.push(metrics.duration);
    if (this.runningAverages.cleanupDuration.length > CheckpointMetricsCollector.MAX_RECENT_SAMPLES) {
      this.runningAverages.cleanupDuration.shift();
    }

    this.emitEvent("cleanup", metrics);
  }

  recordChainLength(metrics: CheckpointChainLengthMetric): void {
    const entityKey = `${metrics.entityId}:chain`;
    const existing = this.metrics.get(entityKey) ?? {};
    const updated = {
      ...existing,
      lastChainLength: metrics.chainLength,
      lastChainLengthTimestamp: metrics.timestamp,
      averageChainLength: existing.averageChainLength
        ? (existing.averageChainLength + metrics.chainLength) / 2
        : metrics.chainLength,
    };
    this.metrics.set(entityKey, updated);

    this.emitEvent("chainLength", metrics);
  }

  getMetrics(entityId: string): any {
    const creationKey = `${entityId}:creation`;
    const loadKey = `${entityId}:load`;
    const cleanupKey = `${entityId}:cleanup`;
    const chainKey = `${entityId}:chain`;

    return {
      ...this.metrics.get(creationKey) ?? {},
      ...this.metrics.get(loadKey) ?? {},
      ...this.metrics.get(cleanupKey) ?? {},
      ...this.metrics.get(chainKey) ?? {},
    };
  }

  getAverageCreationDuration(): number {
    const samples = this.runningAverages.creationDuration;
    if (samples.length === 0) return 0;
    return samples.reduce((sum, val) => sum + val, 0) / samples.length;
  }

  getAverageLoadDuration(): number {
    const samples = this.runningAverages.loadDuration;
    if (samples.length === 0) return 0;
    return samples.reduce((sum, val) => sum + val, 0) / samples.length;
  }

  getAverageCleanupDuration(): number {
    const samples = this.runningAverages.cleanupDuration;
    if (samples.length === 0) return 0;
    return samples.reduce((sum, val) => sum + val, 0) / samples.length;
  }

  getAverageCreationSize(): number {
    const samples = this.runningAverages.creationSize;
    if (samples.length === 0) return 0;
    return samples.reduce((sum, val) => sum + val, 0) / samples.length;
  }

  reset(): void {
    this.metrics.clear();
    this.runningAverages = {
      creationDuration: [],
      loadDuration: [],
      cleanupDuration: [],
      creationSize: [],
    };
  }

  // ============ MetricCollector Interface Implementation ============

  /**
   * Record a metric into the collector.
   * For checkpoint metrics, this is a pass-through — use the
   * specialized recordCreation/recordLoad/recordCleanup methods instead.
   */
  record(metric: Metric): void {
    this.logger.debug("CheckpointMetricsCollector.record called", {
      metricName: metric.metricName,
      metricType: metric.metricType,
    });
  }

  incrementCounter(metricName: string, _labels?: Record<string, string>, _increment?: number): void {
    this.logger.debug("CheckpointMetricsCollector.incrementCounter", { metricName });
  }

  setGauge(metricName: string, value: number, _labels?: Record<string, string>): void {
    this.logger.debug("CheckpointMetricsCollector.setGauge", { metricName, value });
  }

  observeHistogram(metricName: string, value: number, _labels?: Record<string, string>): void {
    this.logger.debug("CheckpointMetricsCollector.observeHistogram", { metricName, value });
  }

  observeSummary(metricName: string, value: number, _labels?: Record<string, string>): void {
    this.logger.debug("CheckpointMetricsCollector.observeSummary", { metricName, value });
  }

  async flush(): Promise<void> {
    // Checkpoint metrics are in-memory only; no-op flush.
  }

  query(_filter: MetricFilter): MetricQueryResult {
    return {
      totalCount: this.metrics.size,
      metrics: new Map<string, AggregatedMetric>(),
      queryTime: 0,
    };
  }

  onReport(_callback: MetricReportCallback, _options?: { interval?: number }): () => void {
    // No periodic reporting for checkpoint metrics
    return () => {};
  }

  clear(): void {
    this.reset();
  }

  dispose(): void {
    this.reset();
    this.listeners = [];
  }

  toPrometheus(): string[] {
    const lines: string[] = [];
    for (const [key, value] of this.metrics.entries()) {
      if (typeof value.lastCreationDuration === "number") {
        lines.push(`checkpoint_creation_duration_ms{entity="${key}"} ${value.lastCreationDuration}`);
      }
      if (typeof value.lastLoadDuration === "number") {
        lines.push(`checkpoint_load_duration_ms{entity="${key}"} ${value.lastLoadDuration}`);
      }
      if (typeof value.totalCreations === "number") {
        lines.push(`checkpoint_creations_total{entity="${key}"} ${value.totalCreations}`);
      }
      if (typeof value.totalLoads === "number") {
        lines.push(`checkpoint_loads_total{entity="${key}"} ${value.totalLoads}`);
      }
    }
    return lines;
  }

  toJSON(): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    for (const [key, value] of this.metrics.entries()) {
      result[key] = value;
    }
    return result;
  }

  // ============ Private Helpers ============

  private emitEvent(
    type: CheckpointMetricsEvent["type"],
    data: CheckpointMetricsEvent["data"],
  ): void {
    const event: CheckpointMetricsEvent = {
      type,
      data,
      timestamp: Date.now(),
    };

    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (error) {
        this.logger.warn("Listener failed for metric event", {
          type,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    if (this.eventEmitter) {
      this.emitMetricEvent(type, data).catch((error) => {
        this.logger.warn("Failed to emit metric event", {
          type,
          error: error instanceof Error ? error.message : String(error),
        });
      });
    }
  }

  private async emitMetricEvent(eventType: string, payload: unknown): Promise<void> {
    try {
      await this.eventEmitter!(eventType, payload);
    } catch (error) {
      this.logger.warn("Failed to emit metric event", {
        eventType,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}