/**
 * Metrics Storage Adapter Interface
 * 
 * Defines the contract for metrics persistence implementations.
 * All storage implementations must be located in packages/storage.
 */

import type { MetricDataPoint, MetricsQuery } from "@wf-agent/common-utils";

// Re-export data point types so existing storage-internal imports continue to work
export { type MetricDataPoint, type MetricsQuery } from "@wf-agent/common-utils";

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
