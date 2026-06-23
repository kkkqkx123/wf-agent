/**
 * Storage Metrics Types
 * Provides visibility into storage performance and usage
 */

/**
 * Storage metrics interface
 * Tracks operation counts, timings, and storage sizes
 */
export interface StorageMetrics {
  // Operation counts
  saveCount: number;
  loadCount: number;
  deleteCount: number;
  listCount: number;

  // Operation timings (milliseconds)
  avgSaveTime: number;
  avgLoadTime: number;
  avgDeleteTime: number;
  avgListTime: number;

  // Storage size
  totalMetadataSize: number;
  totalBlobSize: number;
  totalCount: number;

  // Cache stats (if applicable)
  cacheHitRate?: number;
  cacheSize?: number;
}

/**
 * Default empty metrics
 */
export const DEFAULT_STORAGE_METRICS: StorageMetrics = {
  saveCount: 0,
  loadCount: 0,
  deleteCount: 0,
  listCount: 0,
  avgSaveTime: 0,
  avgLoadTime: 0,
  avgDeleteTime: 0,
  avgListTime: 0,
  totalMetadataSize: 0,
  totalBlobSize: 0,
  totalCount: 0,
};
