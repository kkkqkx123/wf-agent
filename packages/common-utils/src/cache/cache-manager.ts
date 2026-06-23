/**
 * Generic Cache Manager
 * High-performance caching using xxHash (WebAssembly)
 * Supports LRU eviction and TTL
 *
 * Note: Always use createCache() factory function to create instances.
 * Hash algorithm initialization happens automatically.
 */

import { LRUCache } from 'lru-cache';
import { createHashAlgorithm } from './xxhash-algorithm.js';
import type { CacheEntry, CacheStats, CacheConfig, IHashAlgorithm } from './types.js';

export class CacheManager<K = string, V = unknown> {
  private cache: LRUCache<string, CacheEntry<V>>;
  private hashAlgorithm: IHashAlgorithm;
  private config: Required<CacheConfig>;
  private stats: CacheStats;

  constructor(config: CacheConfig, hashAlgorithm: IHashAlgorithm) {
    this.config = this.normalizeConfig(config);
    this.hashAlgorithm = hashAlgorithm;
    this.cache = new LRUCache<string, CacheEntry<V>>({
      max: this.config.maxSize,
    });

    this.stats = {
      size: 0,
      maxSize: this.config.maxSize,
      hits: 0,
      misses: 0,
      hitRate: 0,
      evictions: 0,
      averageAccessTime: 0,
    };
  }

  /**
   * Ensure hash algorithm is initialized
   * @private
   */
  private ensureInitialized(): void {
    if (!this.hashAlgorithm.isInitialized?.()) {
      throw new Error('CacheManager hash algorithm not initialized.');
    }
  }

  /**
   * Get value from cache
   */
  get(key: K): V | null {
    this.ensureInitialized();

    const start = performance.now();
    const hashKey = this.getHashKey(key);

    const entry = this.cache.get(hashKey);

    if (!entry) {
      this.stats.misses++;
      this.updateHitRate();
      return null;
    }

    // Check TTL
    if (entry.ttl && Date.now() - entry.timestamp > entry.ttl) {
      this.cache.delete(hashKey);
      this.stats.size--;
      this.stats.misses++;
      this.updateHitRate();
      return null;
    }

    this.stats.hits++;
    entry.hits++;

    if (this.config.enableStats) {
      const duration = performance.now() - start;
      this.updateAverageAccessTime(duration);
    }

    this.updateHitRate();
    return entry.value;
  }

  /**
   * Set value in cache
   */
  set(key: K, value: V, ttl?: number): void {
    this.ensureInitialized();

    const hashKey = this.getHashKey(key);
    const now = Date.now();

    if (this.cache.has(hashKey)) {
      // Update existing entry
      const entry = this.cache.get(hashKey)!;
      entry.value = value;
      entry.ttl = ttl ?? this.config.ttl;
      entry.timestamp = now;
    } else {
      // New entry - check if we need to evict
      if (this.cache.size >= this.config.maxSize) {
        this.stats.evictions++;
      }

      this.cache.set(hashKey, {
        value,
        timestamp: now,
        ttl: ttl ?? this.config.ttl,
        hits: 0,
        metadata: {},
      });

      this.stats.size = this.cache.size;
    }
  }

  /**
   * Check if key exists in cache
   */
  has(key: K): boolean {
    this.ensureInitialized();

    const hashKey = this.getHashKey(key);
    return this.cache.has(hashKey);
  }

  /**
   * Delete entry from cache
   */
  delete(key: K): void {
    this.ensureInitialized();

    const hashKey = this.getHashKey(key);
    if (this.cache.delete(hashKey)) {
      this.stats.size--;
    }
  }

  /**
   * Clear entire cache
   */
  clear(): void {
    this.cache.clear();
    this.stats.size = 0;
    this.stats.hits = 0;
    this.stats.misses = 0;
    this.stats.evictions = 0;
    this.stats.averageAccessTime = 0;
  }

  /**
   * Get current cache size
   */
  size(): number {
    return this.stats.size;
  }

  /**
   * Get cache statistics
   */
  getStats(): CacheStats {
    return { ...this.stats };
  }

  /**
   * Get hash of a key (for debugging)
   */
  getHashKey(key: K): string {
    return this.hashAlgorithm.hash(key);
  }

  /**
   * Normalize configuration with defaults
   */
  private normalizeConfig(config: CacheConfig): Required<CacheConfig> {
    return {
      maxSize: config.maxSize,
      ttl: config.ttl ?? 0,
      enableStats: config.enableStats ?? true,
      hashBits: config.hashBits ?? 64,
    };
  }

  /**
   * Update average access time
   */
  private updateAverageAccessTime(duration: number): void {
    const total = this.stats.hits + this.stats.misses;
    if (total > 0) {
      this.stats.averageAccessTime =
        (this.stats.averageAccessTime * (total - 1) + duration) / total;
    }
  }

  /**
   * Update cache hit rate
   */
  private updateHitRate(): void {
    const total = this.stats.hits + this.stats.misses;
    this.stats.hitRate = total > 0 ? this.stats.hits / total : 0;
  }
}

/**
 * Factory function for creating and initializing cache managers
 * Automatically initializes hash algorithm and returns ready-to-use cache
 *
 * @example
 * const cache = await createCache({ maxSize: 1000 });
 * const value = cache.get(key);  // ready to use immediately
 */
export async function createCache<K = string, V = unknown>(
  config: CacheConfig
): Promise<CacheManager<K, V>> {
  const hashType = config.hashBits === 32 ? 'xxhash32' : 'xxhash64';
  const hashAlgorithm = createHashAlgorithm(hashType);
  await hashAlgorithm.initialize?.();

  return new CacheManager<K, V>(config, hashAlgorithm);
}
