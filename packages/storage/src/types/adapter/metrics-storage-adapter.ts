/**
 * Metrics Storage Adapter Interface
 * 
 * Defines the contract for metrics persistence implementations.
 * All storage implementations must be located in packages/storage.
 */

/**
 * Metric data point structure
 */
export interface MetricDataPoint {
  /** Metric name (e.g., "workflow.execution.count") */
  metricName: string;
  /** Metric type */
  metricType: 'counter' | 'gauge' | 'histogram';
  /** Metric value */
  value: number;
  /** Timestamp in milliseconds */
  timestamp: number;
  /** Optional labels for filtering */
  labels?: Record<string, string>;
  /** Collector name that generated this metric */
  collectorName: string;
}

/**
 * Metrics query parameters
 */
export interface MetricsQuery {
  /** Filter by metric name */
  metricName?: string;
  /** Filter by metric type */
  metricType?: 'counter' | 'gauge' | 'histogram';
  /** Time range start (Unix timestamp in milliseconds) */
  startTime?: number;
  /** Time range end (Unix timestamp in milliseconds) */
  endTime?: number;
  /** Filter by labels (all must match) */
  labels?: Record<string, string>;
  /** Filter by collector name */
  collectorName?: string;
  /** Maximum number of results */
  limit?: number;
  /** Sort order: 'asc' (oldest first) or 'desc' (newest first) */
  sortOrder?: 'asc' | 'desc';
}

/**
 * Metrics storage adapter interface
 * 
 * Provides persistence capabilities for metrics data.
 * Implementations should be added to packages/storage/src/
 */
export interface MetricsStorageAdapter {
  /**
   * Initialize the storage adapter
   * Creates necessary tables and indexes
   */
  initialize(): Promise<void>;

  /**
   * Save a batch of metric data points
   * @param metrics Array of metrics to persist
   */
  saveBatch(metrics: MetricDataPoint[]): Promise<void>;

  /**
   * Query metrics by criteria
   * @param query Query parameters
   * @returns Matching metric data points
   */
  query(query: MetricsQuery): Promise<MetricDataPoint[]>;

  /**
   * Delete metrics older than specified time
   * @param beforeTimestamp Delete metrics before this timestamp
   * @returns Number of deleted records
   */
  deleteOldMetrics(beforeTimestamp: number): Promise<number>;

  /**
   * Close the storage adapter and release resources
   */
  close(): Promise<void>;
}
