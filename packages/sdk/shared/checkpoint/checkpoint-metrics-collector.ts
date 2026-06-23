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

  getMetrics(entityId: string): CheckpointCreationMetrics[] {
    return this.creationMetrics.get(entityId) || [];
  }

  getAggregate(entityId: string): CheckpointMetricsAggregate | null {
    const metrics = this.getMetrics(entityId);
    if (metrics.length === 0) {
      return null;
    }

    const fullCheckpoints = metrics.filter((m) => m.type === "FULL").length;
    const deltaCheckpoints = metrics.filter((m) => m.type === "DELTA").length;
    const failureCount = metrics.filter((m) => !m.success).length;

    const durations = metrics.map((m) => m.duration);
    const avgCreationTime = durations.reduce((a, b) => a + b, 0) / durations.length;

    const sizes = metrics.filter((m) => m.success).map((m) => m.size);
    const totalSize = sizes.reduce((a, b) => a + b, 0);
    const avgSize = sizes.length > 0 ? totalSize / sizes.length : 0;

    return {
      entityId,
      totalCheckpoints: metrics.length,
      fullCheckpoints,
      deltaCheckpoints,
      avgCreationTime,
      maxCreationTime: Math.max(...durations),
      minCreationTime: Math.min(...durations),
      totalSize,
      avgSize,
      failureCount,
      successRate: ((metrics.length - failureCount) / metrics.length) * 100,
      periodStart: metrics[0]!.timestamp,
      periodEnd: metrics[metrics.length - 1]!.timestamp,
    };
  }

  clearMetrics(entityId: string): void {
    this.creationMetrics.delete(entityId);
    this.cleanupMetrics.delete(entityId);
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
   * Get metrics for an entity
   */
  getMetrics(entityId: string): CheckpointCreationMetrics[] {
    return this.storage.getMetrics(entityId);
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
