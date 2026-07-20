/**
 * Storage Adapter Base Class
 *
 * Provides shared infrastructure (metrics, initialization guard, batch templates, optional data cache)
 * for all storage backend implementations.
 *
 * @template TMetadata - Metadata type
 * @template TListOptions - List query option type
 * @template TSaveOptions - Save operation options type (optional)
 */
import type { BaseStorageAdapter } from "./base-storage-adapter.js";
import type { StorageMetrics } from "../metrics.js";
import { DEFAULT_STORAGE_METRICS } from "../metrics.js";
import { StorageError } from "../storage-errors.js";
import type { ICache } from "@wf-agent/common-utils/cache";

/**
 * Data cache configuration for storage adapters.
 * When enabled, loaded data (Uint8Array) is cached in memory to reduce
 * redundant database queries across all backends (SQLite, PostgreSQL, Memory).
 *
 * Cache is automatically invalidated on save() and delete() for the affected id.
 */
export interface StorageCacheConfig {
  /** Maximum number of entries in the cache (default: 500) */
  maxSize?: number;
  /** Time-to-live in milliseconds (default: 60000 = 1 minute, 0 = no TTL) */
  ttl?: number;
}

export abstract class StorageAdapterBase<TMetadata, TListOptions = Record<string, unknown>, TSaveOptions = void>
  implements BaseStorageAdapter<TMetadata, TListOptions, TSaveOptions>
{
  protected initialized: boolean = false;
  protected metrics: StorageMetrics = { ...DEFAULT_STORAGE_METRICS };

  /**
   * Optional in-memory data cache for loaded Uint8Array data.
   * Shared across all backends — set via configureCache().
   * Cache is automatically invalidated on save() and delete().
   */
  protected cache: ICache<string, Uint8Array> | null = null;

  /**
   * Internal cache hit/miss counters for metrics.
   */
  private cacheHits: number = 0;
  private cacheMisses: number = 0;

  // ── Lifecycle ──────────────────────────────────────────────────────────
  abstract initialize(): Promise<void>;
  abstract close(): Promise<void>;
  abstract clear(): Promise<void>;

  // ── CRUD ───────────────────────────────────────────────────────────────
  abstract save(id: string, data: Uint8Array, metadata: TMetadata, options?: TSaveOptions): Promise<void>;
  abstract load(id: string): Promise<Uint8Array | null>;
  abstract delete(id: string): Promise<void>;
  abstract exists(id: string): Promise<boolean>;
  abstract list(options?: TListOptions): Promise<string[]>;
  abstract getMetadata(id: string): Promise<TMetadata | null>;

  // ── Metrics ────────────────────────────────────────────────────────────
  abstract getMetrics(): Promise<StorageMetrics>;

  resetMetrics(): void {
    this.metrics = { ...DEFAULT_STORAGE_METRICS };
    this.cacheHits = 0;
    this.cacheMisses = 0;
  }

  protected updateMetric(operation: string, timeMs: number, dataSize?: number): void {
    const countKey = `${operation}Count` as keyof StorageMetrics;
    const timeKey = `avg${operation.charAt(0).toUpperCase()}${operation.slice(1)}Time` as keyof StorageMetrics;

    this.metrics[countKey] = (this.metrics[countKey] as number) + 1;

    const currentAvg = this.metrics[timeKey] as number;
    const count = this.metrics[countKey] as number;
    this.metrics[timeKey] = currentAvg + (timeMs - currentAvg) / count;

    if (dataSize !== undefined) {
      this.metrics.totalBlobSize += dataSize;
    }
  }

  // ── Cache support ──────────────────────────────────────────────────────

  /**
   * Configure the optional data cache.
   * Call this during initialize() or after construction.
   * @param config - Cache configuration (null/undefined = disable cache)
   */
  protected configureCache(config?: StorageCacheConfig | null): void {
    if (!config) {
      this.cache = null;
      return;
    }
    // Use a simple Map-based cache with LRU-like eviction by size limit
    const maxSize = config.maxSize ?? 500;
    const ttl = config.ttl ?? 60_000;
    const store = new Map<string, { value: Uint8Array; expiresAt: number }>();

    this.cache = {
      get(key: string): Uint8Array | null {
        const entry = store.get(key);
        if (!entry) return null;
        if (entry.expiresAt !== 0 && Date.now() > entry.expiresAt) {
          store.delete(key);
          return null;
        }
        // Move to end (most recently used)
        store.delete(key);
        store.set(key, entry);
        return entry.value;
      },
      set(key: string, value: Uint8Array, customTtl?: number): void {
        // Evict oldest entry if at capacity
        if (store.size >= maxSize) {
          const oldestKey = store.keys().next().value;
          if (oldestKey !== undefined) store.delete(oldestKey);
        }
        const expiresAt = customTtl ?? ttl;
        store.set(key, {
          value,
          expiresAt: expiresAt > 0 ? Date.now() + expiresAt : 0,
        });
      },
      has(key: string): boolean {
        return this.get(key) !== null;
      },
      delete(key: string): void {
        store.delete(key);
      },
      clear(): void {
        store.clear();
      },
      size(): number {
        return store.size;
      },
      getStats() {
        return {
          size: store.size,
          maxSize,
          hits: 0,
          misses: 0,
          hitRate: 0,
          evictions: 0,
          averageAccessTime: 0,
        };
      },
    };
  }

  /**
   * Load data through the cache (read-through).
   * If cache is enabled and contains the key, returns cached data.
   * Otherwise calls loadFn(), stores the result in cache, and returns it.
   *
   * @param id - The data identifier
   * @param loadFn - The actual database load function to call on cache miss
   * @returns The loaded data or null if not found
   */
  protected async loadFromCache(id: string, loadFn: () => Promise<Uint8Array | null>): Promise<Uint8Array | null> {
    if (!this.cache) {
      return loadFn();
    }

    const cached = this.cache.get(id);
    if (cached !== null) {
      this.cacheHits++;
      // Return a copy to prevent external mutation
      return new Uint8Array(cached);
    }

    this.cacheMisses++;
    const data = await loadFn();
    if (data !== null) {
      this.cache.set(id, data);
    }
    return data;
  }

  /**
   * Save data and invalidate the cache entry for the given id.
   * Call this from save() implementations instead of directly saving.
   *
   * @param id - The data identifier
   * @param saveFn - The actual database save function
   */
  protected async saveAndInvalidateCache(id: string, saveFn: () => Promise<void>): Promise<void> {
    await saveFn();
    this.cache?.delete(id);
  }

  /**
   * Delete data and invalidate the cache entry for the given id.
   * Call this from delete() implementations instead of directly deleting.
   *
   * @param id - The data identifier
   * @param deleteFn - The actual database delete function
   */
  protected async deleteAndInvalidateCache(id: string, deleteFn: () => Promise<void>): Promise<void> {
    await deleteFn();
    this.cache?.delete(id);
  }

  /**
   * Clear the data cache entirely.
   */
  protected clearCache(): void {
    this.cache?.clear();
  }

  /**
   * Populate cache metrics into the metrics object.
   * Call from getMetrics() implementations.
   */
  protected populateCacheMetrics(metrics: StorageMetrics): StorageMetrics {
    const total = this.cacheHits + this.cacheMisses;
    if (total > 0) {
      metrics.cacheHitRate = this.cacheHits / total;
    }
    metrics.cacheSize = this.cache?.size() ?? 0;
    return metrics;
  }

  // ── Guard ──────────────────────────────────────────────────────────────
  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw new StorageError("Storage not initialized. Call initialize() first.", "initialize");
    }
  }

  // ── Batch operations (default implementations, override for optimization) ─
  async saveBatch(
    items: Array<{ id: string; data: Uint8Array; metadata: TMetadata }>,
  ): Promise<void> {
    for (const item of items) {
      await this.save(item.id, item.data, item.metadata);
    }
  }

  async loadBatch(ids: string[]): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    return Promise.all(ids.map(async (id) => ({
      id,
      data: await this.load(id),
    })));
  }

  async deleteBatch(ids: string[]): Promise<void> {
    for (const id of ids) {
      await this.delete(id);
    }
  }
}
