/**
 * Cache Manager - Generic Caching Solution
 * Shared utility for multiple modules
 */

/**
 * Cache entry structure
 */
export interface CacheEntry<T> {
  value: T;
  timestamp: number;
  ttl?: number; // Time to live in ms
  hits: number;
  metadata?: Record<string, unknown>;
}

/**
 * Cache statistics
 */
export interface CacheStats {
  size: number;
  maxSize: number;
  hits: number;
  misses: number;
  hitRate: number;
  evictions: number;
  averageAccessTime: number;
}

/**
 * Hash algorithm interface
 */
export interface IHashAlgorithm {
  hash(input: unknown): string;
  name: string;
  initialize?(): Promise<void>;
  isInitialized?(): boolean;
}

/**
 * Cache configuration
 */
export interface CacheConfig {
  maxSize: number;
  ttl?: number; // Default TTL in ms
  enableStats?: boolean;
  hashBits?: 32 | 64; // xxHash bits: 32 or 64
}

/**
 * Generic cache interface
 */
export interface ICache<K, V> {
  get(key: K): V | null;
  set(key: K, value: V, ttl?: number): void;
  has(key: K): boolean;
  delete(key: K): void;
  clear(): void;
  size(): number;
  getStats(): CacheStats;
}
