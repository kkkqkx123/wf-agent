/**
 * Checkpoint Metrics Collector
 *
 * Collects and manages checkpoint operation metrics including:
 * - Creation time and size
 * - Cleanup operations
 * - Aggregated statistics
 */

import type {
  CheckpointCreationMetrics,
  CheckpointCleanupMetrics,
  CheckpointLoadMetrics,
  CheckpointChainLengthMetric,
  CheckpointMetricsAggregate,
  CheckpointMetricsConfig,
  ICheckpointMetricsStorage,
  CheckpointMetricsEvent,
} from "@wf-agent/types";

interface Logger {
  debug(msg: string, context?: Record<string, unknown>): void;
  info(msg: string, context?: Record<string, unknown>): void;
}

/**
 * Default metrics storage implementation
 */
class CheckpointMetricsMemoryStorage implements ICheckpointMetricsStorage {
  private creationMetrics: Map<string, CheckpointCreationMetrics[]> = new Map();
  private cleanupMetrics: Map<string, CheckpointCleanupMetrics[]> = new Map();
  private loadMetrics: Map<string, CheckpointLoadMetrics[]> = new Map();
  private chainLengthMetrics: Map<string, CheckpointChainLengthMetric[]> = new Map();
  private maxMetrics: number;

  constructor(maxMetrics: number = 1000) {
    this.maxMetrics = maxMetrics;
  }

  recordCreation(metrics: CheckpointCreationMetrics): void {
    const entityMetrics = this.creationMetrics.get(metrics.entityId) || [];
    entityMetrics.push(metrics);

    if (entityMetrics.length > this.maxMetrics) {
      entityMetrics.shift();
    }

    this.creationMetrics.set(metrics.entityId, entityMetrics);
  }

  recordCleanup(metrics: CheckpointCleanupMetrics): void {
    const entityMetrics = this.cleanupMetrics.get(metrics.entityId) || [];
    entityMetrics.push(metrics);

    if (entityMetrics.length > this.maxMetrics) {
      entityMetrics.shift();
    }

    this.cleanupMetrics.set(metrics.entityId, entityMetrics);
  }

  recordLoad(metrics: CheckpointLoadMetrics): void {
    const entityMetrics = this.loadMetrics.get(metrics.entityId) || [];
    entityMetrics.push(metrics);

    if (entityMetrics.length > this.maxMetrics) {
      entityMetrics.shift();
    }

    this.loadMetrics.set(metrics.entityId, entityMetrics);
  }

  recordChainLength(metrics: CheckpointChainLengthMetric): void {
    const entityMetrics = this.chainLengthMetrics.get(metrics.entityId) || [];
    entityMetrics.push(metrics);

    if (entityMetrics.length > this.maxMetrics) {
      entityMetrics.shift();
    }

    this.chainLengthMetrics.set(metrics.entityId, entityMetrics);
  }

  getMetrics(entityId: string): CheckpointCreationMetrics[] {
    return this.creationMetrics.get(entityId) || [];
  }

  getLoadMetrics(entityId: string): CheckpointLoadMetrics[] {
    return this.loadMetrics.get(entityId) || [];
  }

  getChainLengthMetrics(entityId: string): CheckpointChainLengthMetric[] {
    return this.chainLengthMetrics.get(entityId) || [];
  }

  getAggregate(entityId: string): CheckpointMetricsAggregate | null {
    const metrics = this.getMetrics(entityId);
    const loadMetrics = this.getLoadMetrics(entityId);
    const chainLengthMetrics = this.getChainLengthMetrics(entityId);
    if (metrics.length === 0 && loadMetrics.length === 0) {
      return null;
    }

    const fullCheckpoints = metrics.filter((m) => m.type === "FULL").length;
    const deltaCheckpoints = metrics.filter((m) => m.type === "DELTA").length;
    const failureCount = metrics.filter((m) => !m.success).length;

    const durations = metrics.map((m) => m.duration);
    const avgCreationTime = durations.length > 0 ? durations.reduce((a, b) => a + b, 0) / durations.length : 0;

    const sizes = metrics.filter((m) => m.success).map((m) => m.size);
    const totalSize = sizes.reduce((a, b) => a + b, 0);
    const avgSize = sizes.length > 0 ? totalSize / sizes.length : 0;

    const result: CheckpointMetricsAggregate = {
      entityId,
      totalCheckpoints: metrics.length,
      fullCheckpoints,
      deltaCheckpoints,
      avgCreationTime,
      maxCreationTime: durations.length > 0 ? Math.max(...durations) : 0,
      minCreationTime: durations.length > 0 ? Math.min(...durations) : 0,
      totalSize,
      avgSize,
      failureCount,
      successRate: metrics.length > 0 ? ((metrics.length - failureCount) / metrics.length) * 100 : 100,
      periodStart: metrics.length > 0 ? metrics[0]!.timestamp : 0,
      periodEnd: metrics.length > 0 ? metrics[metrics.length - 1]!.timestamp : 0,
    };

    if (loadMetrics.length > 0) {
      const loadDurations = loadMetrics.filter((m) => m.success).map((m) => m.duration);
      if (loadDurations.length > 0) {
        const sorted = [...loadDurations].sort((a, b) => a - b);
        result.avgLoadTime = loadDurations.reduce((a, b) => a + b, 0) / loadDurations.length;
        result.p50LoadTime = sorted[Math.floor(sorted.length * 0.5)]!;
        result.p95LoadTime = sorted[Math.floor(sorted.length * 0.95)]!;
        result.p99LoadTime = sorted[Math.floor(sorted.length * 0.99)]!;
      }
    }

    if (chainLengthMetrics.length > 0) {
      const lengths = chainLengthMetrics.map((m) => m.chainLength);
      result.maxChainLength = Math.max(...lengths);
      result.avgChainLength = lengths.reduce((a, b) => a + b, 0) / lengths.length;
    }

    return result;
  }

  clearMetrics(entityId: string): void {
    this.creationMetrics.delete(entityId);
    this.cleanupMetrics.delete(entityId);
    this.loadMetrics.delete(entityId);
    this.chainLengthMetrics.delete(entityId);
  }

  getAllMetrics(): CheckpointCreationMetrics[] {
    const all: CheckpointCreationMetrics[] = [];
    this.creationMetrics.forEach((metrics) => all.push(...metrics));
    return all;
  }
}

/**
 * Checkpoint Metrics Collector
 * Collects and manages checkpoint operation metrics
 */
export class CheckpointMetricsCollector {
  private config: CheckpointMetricsConfig;
  private storage: ICheckpointMetricsStorage;
  private eventListeners: ((event: CheckpointMetricsEvent) => void)[] = [];
  private readonly logger: Logger;

  constructor(config: CheckpointMetricsConfig, logger: Logger, storage?: ICheckpointMetricsStorage) {
    this.config = config;
    this.logger = logger;
    this.storage = storage || new CheckpointMetricsMemoryStorage(config.maxMetrics || 1000);
  }

  /**
   * Record checkpoint creation metrics
   */
  recordCreation(metrics: CheckpointCreationMetrics): void {
    if (!this.config.enabled) {
      return;
    }

    this.storage.recordCreation(metrics);

    this.emit({
      type: "creation",
      data: metrics,
      timestamp: Date.now(),
    });

    this.logger.debug("Checkpoint creation metrics recorded", {
      checkpointId: metrics.checkpointId,
      duration: metrics.duration,
      size: metrics.size,
      success: metrics.success,
    });

    if (this.config.autoAggregate) {
      this.emitAggregate(metrics.entityId);
    }
  }

  /**
   * Record checkpoint cleanup metrics
   */
  recordCleanup(metrics: CheckpointCleanupMetrics): void {
    if (!this.config.enabled) {
      return;
    }

    this.storage.recordCleanup(metrics);

    this.emit({
      type: "cleanup",
      data: metrics,
      timestamp: Date.now(),
    });

    this.logger.debug("Checkpoint cleanup metrics recorded", {
      entityId: metrics.entityId,
      count: metrics.count,
      sizeFreed: metrics.sizeFreed,
      duration: metrics.duration,
    });
  }

  /**
   * Record checkpoint load metrics
   */
  recordLoad(metrics: CheckpointLoadMetrics): void {
    if (!this.config.enabled) {
      return;
    }

    this.storage.recordLoad(metrics);

    this.emit({
      type: "creation",
      data: metrics,
      timestamp: Date.now(),
    });

    this.logger.debug("Checkpoint load metrics recorded", {
      checkpointId: metrics.checkpointId,
      duration: metrics.duration,
      success: metrics.success,
    });
  }

  /**
   * Record checkpoint chain length metric
   */
  recordChainLength(metrics: CheckpointChainLengthMetric): void {
    if (!this.config.enabled) {
      return;
    }

    this.storage.recordChainLength(metrics);

    this.emit({
      type: "aggregate",
      data: metrics,
      timestamp: Date.now(),
    });

    this.logger.debug("Checkpoint chain length recorded", {
      entityId: metrics.entityId,
      chainLength: metrics.chainLength,
      fullCount: metrics.fullCount,
      deltaCount: metrics.deltaCount,
    });
  }

  /**
   * Get metrics for an entity
   */
  getMetrics(entityId: string): CheckpointCreationMetrics[] {
    return this.storage.getMetrics(entityId);
  }

  /**
   * Get load metrics for an entity
   */
  getLoadMetrics(entityId: string): CheckpointLoadMetrics[] {
    return this.storage.getLoadMetrics(entityId);
  }

  /**
   * Get chain length metrics for an entity
   */
  getChainLengthMetrics(entityId: string): CheckpointChainLengthMetric[] {
    return this.storage.getChainLengthMetrics(entityId);
  }

  /**
   * Get aggregated metrics for an entity
   */
  getAggregate(entityId: string): CheckpointMetricsAggregate | null {
    return this.storage.getAggregate(entityId);
  }

  /**
   * Clear metrics for an entity
   */
  clearMetrics(entityId: string): void {
    this.storage.clearMetrics(entityId);
    this.logger.debug("Metrics cleared for entity", { entityId });
  }

  /**
   * Get all metrics
   */
  getAllMetrics(): CheckpointCreationMetrics[] {
    return this.storage.getAllMetrics();
  }

  /**
   * Subscribe to metrics events
   */
  on(listener: (event: CheckpointMetricsEvent) => void): void {
    this.eventListeners.push(listener);
  }

  /**
   * Unsubscribe from metrics events
   */
  off(listener: (event: CheckpointMetricsEvent) => void): void {
    this.eventListeners = this.eventListeners.filter((l) => l !== listener);
  }

  /**
   * Emit metrics event
   */
  private emit(event: CheckpointMetricsEvent): void {
    this.eventListeners.forEach((listener) => {
      try {
        listener(event);
      } catch (error) {
        this.logger.debug("Error in metrics event listener", {
          error: error instanceof Error ? error.message : String(error),
        });
      }
    });
  }

  /**
   * Emit aggregated metrics for an entity
   */
  private emitAggregate(entityId: string): void {
    const aggregate = this.storage.getAggregate(entityId);
    if (aggregate) {
      this.emit({
        type: "aggregate",
        data: aggregate,
        timestamp: Date.now(),
      });
    }
  }
}
