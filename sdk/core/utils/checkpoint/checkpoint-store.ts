/**
 * CheckpointStore - Checkpoint Storage
 * Provides generic caching capabilities with TTL support for checkpoint data
 *
 * Responsibilities:
 * - Generic key-value caching for checkpoint data
 * - TTL (Time-To-Live) based expiration
 * - Cache statistics tracking
 * - Memory-efficient storage
 *
 * Design Principles:
 * - Generic type support for any cached value type
 * - Configurable TTL
 * - Simple and focused API
 * - No external dependencies
 */

/**
 * Cache entry structure
 */
interface CacheEntry<T> {
  /** Cached value */
  value: T;
  /** Timestamp when the entry was created */
  timestamp: number;
  /** Optional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Cache statistics
 */
export interface CacheStats {
  /** Total number of entries */
  entries: number;
  /** Number of cache hits */
  hits: number;
  /** Number of cache misses */
  misses: number;
  /** Hit rate (0-1) */
  hitRate: number;
}

/**
 * Cache configuration
 */
export interface CacheConfig {
  /** Time-to-live in milliseconds (default: 5 minutes) */
  ttl?: number;
  /** Maximum number of entries (default: unlimited) */
  maxSize?: number;
}

/**
 * CheckpointStore - Checkpoint Storage Class
 *
 * Provides a simple, generic caching mechanism with TTL support.
 * Can be used for any type of cached content.
 *
 * @example
 * ```typescript
 * const store = new CheckpointStore<string>({ ttl: 60000 }); // 1 minute TTL
 * store.set('key', 'value');
 * const value = store.get('key');
 * ```
 */
export class CheckpointStore<T = unknown> {
  private cache: Map<string, CacheEntry<T>> = new Map();
  private readonly ttl: number;
  private readonly maxSize?: number;
  private hits: number = 0;
  private misses: number = 0;

  /**
   * Constructor
   * @param config Cache configuration
   */
  constructor(config: CacheConfig = {}) {
    this.ttl = config.ttl ?? 300000; // Default: 5 minutes
    this.maxSize = config.maxSize;
  }

  /**
   * Get a value from the cache
   * @param key Cache key
   * @returns Cached value or null if not found or expired
   */
  get(key: string): T | null {
    const entry = this.cache.get(key);

    if (!entry) {
      this.misses++;
      return null;
    }

    // Check if expired
    if (this.isExpired(entry)) {
      this.cache.delete(key);
      this.misses++;
      return null;
    }

    this.hits++;
    return entry.value;
  }

  /**
   * Set a value in the cache
   * @param key Cache key
   * @param value Value to cache
   * @param metadata Optional metadata
   */
  set(key: string, value: T, metadata?: Record<string, unknown>): void {
    // Enforce max size
    if (this.maxSize !== undefined && this.cache.size >= this.maxSize) {
      this.evictOldest();
    }

    this.cache.set(key, {
      value,
      timestamp: Date.now(),
      metadata,
    });
  }

  /**
   * Check if a key exists in the cache (and is not expired)
   * @param key Cache key
   * @returns True if the key exists and is not expired
   */
  has(key: string): boolean {
    const entry = this.cache.get(key);

    if (!entry) {
      return false;
    }

    if (this.isExpired(entry)) {
      this.cache.delete(key);
      return false;
    }

    return true;
  }

  /**
   * Delete a key from the cache
   * @param key Cache key
   * @returns True if the key was deleted
   */
  delete(key: string): boolean {
    return this.cache.delete(key);
  }

  /**
   * Clear all entries from the cache
   */
  clear(): void {
    this.cache.clear();
    this.hits = 0;
    this.misses = 0;
  }

  /**
   * Get the number of entries in the cache
   * @returns Number of entries
   */
  get size(): number {
    return this.cache.size;
  }

  /**
   * Get cache statistics
   * @returns Cache statistics
   */
  getStats(): CacheStats {
    const total = this.hits + this.misses;
    return {
      entries: this.cache.size,
      hits: this.hits,
      misses: this.misses,
      hitRate: total > 0 ? this.hits / total : 0,
    };
  }

  /**
   * Get all keys in the cache
   * @returns Array of cache keys
   */
  keys(): string[] {
    return Array.from(this.cache.keys());
  }

  /**
   * Clean up expired entries
   * @returns Number of entries removed
   */
  cleanup(): number {
    let removed = 0;
    for (const [key, entry] of this.cache.entries()) {
      if (this.isExpired(entry)) {
        this.cache.delete(key);
        removed++;
      }
    }
    return removed;
  }

  /**
   * Get or set a value
   * If the key exists, returns the cached value.
   * If not, calls the factory function, caches the result, and returns it.
   *
   * @param key Cache key
   * @param factory Factory function to create the value if not cached
   * @returns Cached or newly created value
   */
  async getOrSet(key: string, factory: () => Promise<T>): Promise<T> {
    const cached = this.get(key);
    if (cached !== null) {
      return cached;
    }

    const value = await factory();
    this.set(key, value);
    return value;
  }

  // ============================================================
  // Private methods
  // ============================================================

  /**
   * Check if a cache entry is expired
   * @param entry Cache entry
   * @returns True if expired
   */
  private isExpired(entry: CacheEntry<T>): boolean {
    return Date.now() - entry.timestamp > this.ttl;
  }

  /**
   * Evict the oldest entry from the cache
   */
  private evictOldest(): void {
    let oldestKey: string | null = null;
    let oldestTimestamp = Infinity;

    for (const [key, entry] of this.cache.entries()) {
      if (entry.timestamp < oldestTimestamp) {
        oldestTimestamp = entry.timestamp;
        oldestKey = key;
      }
    }

    if (oldestKey !== null) {
      this.cache.delete(oldestKey);
    }
  }
}
