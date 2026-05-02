/**
 * In-Memory Storage Base Class
 * Fast, isolated storage implementation perfect for testing
 */

import type { BaseStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { StorageError } from "../types/storage-errors.js";
import { createModuleLogger } from "../logger.js";
import type { StorageMetrics } from "../types/metrics.js";
import { DEFAULT_STORAGE_METRICS } from "../types/metrics.js";

const logger = createModuleLogger("memory-storage");

/**
 * Memory Storage Configuration
 */
export interface MemoryStorageConfig {
  /** Simulate I/O latency for realistic testing (default: false) */
  simulateLatency?: boolean;
  /** Latency in milliseconds when simulating (default: 10) */
  latencyMs?: number;
  /** Randomly simulate errors for resilience testing (default: false) */
  simulateErrors?: boolean;
  /** Error probability 0-1 when simulating errors (default: 0.01) */
  errorRate?: number;
}

/**
 * Base in-memory storage implementation
 * Provides fast, isolated storage with optional latency/error simulation
 * @template TMetadata Metadata type
 * @template TListOptions List options type
 */
export abstract class BaseMemoryStorage<TMetadata, TListOptions = any>
  implements BaseStorageAdapter<TMetadata, TListOptions>
{
  protected store: Map<string, { data: Uint8Array; metadata: TMetadata }> = new Map();
  protected initialized: boolean = false;
  protected config: Required<MemoryStorageConfig>;
  protected metrics: StorageMetrics = { ...DEFAULT_STORAGE_METRICS };

  constructor(config: MemoryStorageConfig = {}) {
    this.config = {
      simulateLatency: config.simulateLatency ?? false,
      latencyMs: config.latencyMs ?? 10,
      simulateErrors: config.simulateErrors ?? false,
      errorRate: config.errorRate ?? 0.01,
    };
  }

  /**
   * Initialize memory storage
   */
  async initialize(): Promise<void> {
    logger.debug("Initializing memory storage");
    this.initialized = true;
  }

  /**
   * Save data to memory storage
   */
  async save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void> {
    const startTime = Date.now();
    this.ensureInitialized();
    await this.simulateLatency();
    await this.simulateRandomError("save");

    // Create a copy to prevent external mutation
    this.store.set(id, { data: new Uint8Array(data), metadata });
    
    const elapsed = Date.now() - startTime;
    this.updateMetric('save', elapsed, data.length);
    logger.debug("Data saved to memory storage", { id, size: data.length });
  }

  /**
   * Load data from memory storage
   */
  async load(id: string): Promise<Uint8Array | null> {
    const startTime = Date.now();
    this.ensureInitialized();
    await this.simulateLatency();
    await this.simulateRandomError("load");

    const entry = this.store.get(id);
    if (!entry) {
      logger.debug("Data not found in memory storage", { id });
      return null;
    }

    // Return a copy to prevent external mutation
    const elapsed = Date.now() - startTime;
    this.updateMetric('load', elapsed, entry.data.length);
    logger.debug("Data loaded from memory storage", { id, size: entry.data.length });
    return new Uint8Array(entry.data);
  }

  /**
   * Delete data from memory storage
   */
  async delete(id: string): Promise<void> {
    const startTime = Date.now();
    this.ensureInitialized();
    await this.simulateLatency();
    await this.simulateRandomError("delete");

    this.store.delete(id);
    
    const elapsed = Date.now() - startTime;
    this.updateMetric('delete', elapsed);
    logger.debug("Data deleted from memory storage", { id });
  }

  /**
   * Check if data exists in memory storage
   */
  async exists(id: string): Promise<boolean> {
    this.ensureInitialized();
    await this.simulateLatency();
    return this.store.has(id);
  }

  /**
   * List all IDs in memory storage
   */
  async list(options?: TListOptions): Promise<string[]> {
    const startTime = Date.now();
    this.ensureInitialized();
    await this.simulateLatency();
    
    const result = Array.from(this.store.keys());
    const elapsed = Date.now() - startTime;
    this.updateMetric('list', elapsed);
    
    return result;
  }

  /**
   * Clear all data from memory storage
   */
  async clear(): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();

    const count = this.store.size;
    this.store.clear();
    logger.info("Memory storage cleared", { removedItems: count });
  }

  /**
   * Close memory storage (clears all data)
   */
  async close(): Promise<void> {
    logger.debug("Closing memory storage");
    this.store.clear();
    this.initialized = false;
  }

  /**
   * Get metadata without loading data
   */
  async getMetadata(id: string): Promise<TMetadata | null> {
    this.ensureInitialized();
    const entry = this.store.get(id);
    return entry?.metadata ?? null;
  }

  /**
   * Get storage metrics
   */
  async getMetrics(): Promise<StorageMetrics> {
    let totalSize = 0;
    for (const entry of this.store.values()) {
      totalSize += entry.data.length;
    }

    return {
      ...this.metrics,
      totalCount: this.store.size,
      totalBlobSize: totalSize,
    };
  }

  /**
   * Reset metrics counters
   */
  resetMetrics(): void {
    this.metrics = { ...DEFAULT_STORAGE_METRICS };
  }

  /**
   * Update metrics for an operation
   */
  protected updateMetric(operation: string, timeMs: number, dataSize?: number): void {
    const countKey = `${operation}Count` as keyof StorageMetrics;
    const timeKey = `avg${operation.charAt(0).toUpperCase()}${operation.slice(1)}Time` as keyof StorageMetrics;

    this.metrics[countKey] = (this.metrics[countKey] as number) + 1;

    // Running average calculation
    const currentAvg = this.metrics[timeKey] as number;
    const count = this.metrics[countKey] as number;
    this.metrics[timeKey] = currentAvg + (timeMs - currentAvg) / count;

    if (dataSize !== undefined) {
      this.metrics.totalBlobSize += dataSize;
    }
  }

  /**
   * Get storage statistics
   */
  getStats(): { count: number; totalSize: number } {
    let totalSize = 0;
    for (const entry of this.store.values()) {
      totalSize += entry.data.length;
    }

    return {
      count: this.store.size,
      totalSize,
    };
  }

  /**
   * Ensure storage is initialized
   */
  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw new StorageError("Memory storage not initialized. Call initialize() first.", "initialize");
    }
  }

  /**
   * Simulate I/O latency if configured
   */
  protected async simulateLatency(): Promise<void> {
    if (this.config.simulateLatency) {
      await new Promise(resolve => setTimeout(resolve, this.config.latencyMs));
    }
  }

  /**
   * Simulate random errors if configured
   */
  private async simulateRandomError(operation: string): Promise<void> {
    if (this.config.simulateErrors && Math.random() < this.config.errorRate) {
      throw new StorageError(
        `Simulated random error during ${operation}`,
        operation,
        { simulated: true, operation },
      );
    }
  }

  /**
   * Save multiple items in a single operation
   * More efficient than individual saves for bulk operations
   * @param items Array of items to save with id, data, and metadata
   */
  async saveBatch(
    items: Array<{ id: string; data: Uint8Array; metadata: TMetadata }>,
  ): Promise<void> {
    const startTime = Date.now();
    this.ensureInitialized();
    await this.simulateLatency();

    logger.debug("Starting batch save", { count: items.length });

    try {
      // Save all items directly to the Map
      for (const item of items) {
        await this.simulateRandomError("saveBatch");
        this.store.set(item.id, {
          data: item.data,
          metadata: item.metadata,
        });
      }

      const elapsed = Date.now() - startTime;
      const totalSize = items.reduce((sum, item) => sum + item.data.length, 0);
      this.updateMetric('save', elapsed / items.length, totalSize);

      logger.debug("Batch save completed", {
        count: items.length,
        totalTimeMs: elapsed,
      });
    } catch (error) {
      logger.error("Batch save failed", {
        error: (error as Error).message,
      });
      throw error;
    }
  }

  /**
   * Load multiple items efficiently
   * @param ids Array of IDs to load
   * @returns Array of loaded data (null if not found), maintaining order
   */
  async loadBatch(ids: string[]): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    const startTime = Date.now();
    this.ensureInitialized();
    await this.simulateLatency();

    if (ids.length === 0) {
      return [];
    }

    logger.debug("Starting batch load", { count: ids.length });

    try {
      // Load all items from the Map
      const results = ids.map(id => {
        const entry = this.store.get(id);
        return {
          id,
          data: entry ? entry.data : null,
        };
      });

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed / ids.length);

      logger.debug("Batch load completed", {
        requested: ids.length,
        found: results.filter(r => r.data !== null).length,
        totalTimeMs: elapsed,
      });

      return results;
    } catch (error) {
      logger.error("Batch load failed", {
        error: (error as Error).message,
      });
      throw error;
    }
  }

  /**
   * Delete multiple items in a single operation
   * More efficient than individual deletes for bulk operations
   * @param ids Array of IDs to delete
   */
  async deleteBatch(ids: string[]): Promise<void> {
    const startTime = Date.now();
    this.ensureInitialized();
    await this.simulateLatency();

    if (ids.length === 0) {
      return;
    }

    logger.debug("Starting batch delete", { count: ids.length });

    try {
      // Delete all items from the Map
      for (const id of ids) {
        await this.simulateRandomError("deleteBatch");
        this.store.delete(id);
      }

      const elapsed = Date.now() - startTime;
      this.updateMetric('delete', elapsed / ids.length);

      logger.debug("Batch delete completed", {
        count: ids.length,
        totalTimeMs: elapsed,
      });
    } catch (error) {
      logger.error("Batch delete failed", {
        error: (error as Error).message,
      });
      throw error;
    }
  }
}
