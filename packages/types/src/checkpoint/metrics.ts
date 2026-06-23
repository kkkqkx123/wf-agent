/**
 * Checkpoint Metrics Collection and Monitoring
 *
 * Provides types and interfaces for collecting performance metrics
 * on checkpoint operations including creation time, size, and cleanup.
 */

/**
 * Checkpoint creation metrics
 */
export interface CheckpointCreationMetrics {
  /** Checkpoint ID */
  checkpointId: string;
  /** Parent entity ID (agentLoopId or executionId) */
  entityId: string;
  /** Checkpoint type (FULL or DELTA) */
  type: "FULL" | "DELTA";
  /** Time taken to create checkpoint in milliseconds */
  duration: number;
  /** Size of checkpoint data in bytes */
  size: number;
  /** Timestamp when checkpoint was created */
  timestamp: number;
  /** Whether checkpoint was successfully created */
  success: boolean;
  /** Error message if creation failed */
  error?: string;
}

/**
 * Checkpoint cleanup metrics
 */
export interface CheckpointCleanupMetrics {
  /** Parent entity ID */
  entityId: string;
  /** Number of checkpoints cleaned up */
  count: number;
  /** Total size freed in bytes */
  sizeFreed: number;
  /** Time taken to cleanup in milliseconds */
  duration: number;
  /** Timestamp when cleanup occurred */
  timestamp: number;
  /** Whether cleanup was successful */
  success: boolean;
  /** Error message if cleanup failed */
  error?: string;
}

/**
 * Aggregated checkpoint metrics for an entity
 */
export interface CheckpointMetricsAggregate {
  /** Parent entity ID */
  entityId: string;
  /** Total number of checkpoints created */
  totalCheckpoints: number;
  /** Number of FULL checkpoints */
  fullCheckpoints: number;
  /** Number of DELTA checkpoints */
  deltaCheckpoints: number;
  /** Average creation time in milliseconds */
  avgCreationTime: number;
  /** Max creation time in milliseconds */
  maxCreationTime: number;
  /** Min creation time in milliseconds */
  minCreationTime: number;
  /** Total size of all checkpoints in bytes */
  totalSize: number;
  /** Average checkpoint size in bytes */
  avgSize: number;
  /** Number of failed checkpoints */
  failureCount: number;
  /** Success rate (0-100) */
  successRate: number;
  /** Metrics collection period start timestamp */
  periodStart: number;
  /** Metrics collection period end timestamp */
  periodEnd: number;
}

/**
 * Metrics configuration
 */
export interface CheckpointMetricsConfig {
  /** Enable metrics collection */
  enabled: boolean;
  /** Maximum number of metrics to retain per entity */
  maxMetrics?: number;
  /** Enable automatic aggregation */
  autoAggregate?: boolean;
}

/**
 * Metrics event that can be emitted
 */
export interface CheckpointMetricsEvent {
  /** Event type */
  type: "creation" | "cleanup" | "aggregate";
  /** Associated metrics data */
  data: CheckpointCreationMetrics | CheckpointCleanupMetrics | CheckpointMetricsAggregate;
  /** Event timestamp */
  timestamp: number;
}

/**
 * Checkpoint metrics storage interface
 */
export interface ICheckpointMetricsStorage {
  /** Record creation metrics */
  recordCreation(metrics: CheckpointCreationMetrics): void;
  /** Record cleanup metrics */
  recordCleanup(metrics: CheckpointCleanupMetrics): void;
  /** Get metrics for an entity */
  getMetrics(entityId: string): CheckpointCreationMetrics[];
  /** Get aggregated metrics for an entity */
  getAggregate(entityId: string): CheckpointMetricsAggregate | null;
  /** Clear metrics for an entity */
  clearMetrics(entityId: string): void;
  /** Get all metrics */
  getAllMetrics(): CheckpointCreationMetrics[];
}
