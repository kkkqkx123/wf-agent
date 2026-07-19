/**
 * PostgreSQL Connection Pool Manager
 * Manages PostgreSQL connection pools with global sharing support
 */

import { Pool, type PoolConfig } from 'pg';
import { createModuleLogger } from '../logger.js';

const logger = createModuleLogger('postgres-pool');

/**
 * PostgreSQL Pool Configuration
 */
export interface PostgresPoolConfig extends PoolConfig {
  /** Maximum number of connections in the pool (default: 20) */
  max?: number;
  /** Minimum number of idle connections (default: 1) */
  min?: number;
  /** Idle timeout in milliseconds (default: 30000) */
  idleTimeoutMillis?: number;
  /** Connection timeout in milliseconds (default: 5000) */
  connectionTimeoutMillis?: number;
  /** Maximum number of times a connection can be used before being recycled (default: Infinity) */
  maxUses?: number;
  /** Connection health check function (pg-pool runtime option, called on each new connection) */
  verify?: (client: any, done: (err?: Error) => void) => void;
}

/**
 * Advanced pool metrics
 */
export interface PoolMetrics {
  /** Number of times a pool hit was returned */
  hits: number;
  /** Number of times a new pool was created */
  misses: number;
  /** Total acquire time in milliseconds */
  totalAcquireTime: number;
  /** Number of acquire operations tracked */
  acquireCount: number;
  /** Average acquire time in milliseconds */
  avgAcquireTime: number;
  /** Pool hit rate (0-1) */
  hitRate: number;
}

/**
 * Pool statistics
 */
export interface PoolStats {
  totalCount: number;
  idleCount: number;
  waitingCount: number;
  max: number;
  metrics: PoolMetrics;
}

/** Internal pool metadata */
interface PoolEntry {
  pool: Pool;
  config: PostgresPoolConfig;
  metrics: PoolMetrics;
  lastAccessedAt: number;
}

/**
 * Options for PostgresConnectionPool
 */
export interface PostgresConnectionPoolOptions {
  /** Maximum number of connection pools (default: 50) */
  maxPools?: number;
  /** Idle pool cleanup interval in milliseconds (default: 60000, 0 to disable) */
  idlePoolCleanupMs?: number;
}

/**
 * PostgreSQL Connection Pool
 * Manages connection pools per database connection string
 */
export class PostgresConnectionPool {
  private pools: Map<string, PoolEntry> = new Map();
  private readonly maxPools: number;
  private readonly idlePoolCleanupMs: number;
  private cleanupTimer: ReturnType<typeof setInterval> | null = null;

  constructor(options?: PostgresConnectionPoolOptions) {
    this.maxPools = options?.maxPools ?? 50;
    this.idlePoolCleanupMs = options?.idlePoolCleanupMs ?? 60000;

    if (this.idlePoolCleanupMs > 0) {
      this.cleanupTimer = setInterval(() => {
        this.cleanupIdlePools().catch((err) => {
          logger.error('Idle pool cleanup failed', { error: sanitizeErrorMessage(err) });
        });
      }, this.idlePoolCleanupMs);
      // Allow the process to exit even if the timer is still active
      if (this.cleanupTimer.unref) {
        this.cleanupTimer.unref();
      }
    }
  }

  /**
   * Get or create a connection pool for the given connection string
   * @param connectionString PostgreSQL connection string
   * @param config Optional pool configuration
   * @returns Pool instance
   */
  getPool(connectionString: string, config?: PostgresPoolConfig): Pool {
    const sanitizedConnStr = sanitizeConnectionString(connectionString);

    // Check if pool already exists
    const existingEntry = this.pools.get(connectionString);
    if (existingEntry) {
      existingEntry.lastAccessedAt = Date.now();
      existingEntry.metrics.hits++;
      logger.debug('Reusing existing connection pool', { connectionString: sanitizedConnStr });
      return existingEntry.pool;
    }

    // Enforce max pools limit — evict the least recently used pool if necessary
    if (this.pools.size >= this.maxPools) {
      logger.warn('Maximum pool count reached, evicting LRU pool', {
        current: this.pools.size,
        max: this.maxPools,
      });
      this.evictLRUPool();
    }

    // Create new pool with default or provided config
    const poolConfig: PostgresPoolConfig = {
      max: 20,
      min: 1,
      idleTimeoutMillis: 30000,
      connectionTimeoutMillis: 5000,
      ...config,
    };

    logger.info('Creating new connection pool', {
      connectionString: sanitizedConnStr,
      config: {
        max: poolConfig.max,
        min: poolConfig.min,
        idleTimeoutMillis: poolConfig.idleTimeoutMillis,
      },
    });

    // Enable connection health check via pg-pool's built-in verify option
    const verifyFn = poolConfig.verify ?? ((client: any, done: (err?: Error) => void) => {
      client.query('SELECT 1', (err: Error | null) => {
        if (err) {
          logger.warn('Connection health check failed, recycling client', {
            connectionString: sanitizedConnStr,
            error: err.message,
          });
        }
        done(err ?? undefined);
      });
    });

    // Split known PoolConfig properties from runtime-only options (verify)
    const { verify: _verify, ...poolOptions } = poolConfig;
    const pool = new Pool({
      connectionString,
      ...poolOptions,
      verify: verifyFn,
    } as any);

    // Set up event listeners for monitoring
    pool.on('error', (err) => {
      logger.error('Unexpected pool error', {
        connectionString: sanitizedConnStr,
        error: err.message,
      });
    });

    pool.on('connect', () => {
      logger.debug('New client connected to pool', { connectionString: sanitizedConnStr });
    });

    pool.on('acquire', () => {
      logger.debug('Client acquired from pool', { connectionString: sanitizedConnStr });
    });

    pool.on('remove', () => {
      logger.debug('Client removed from pool', { connectionString: sanitizedConnStr });
    });

    // Create metrics entry
    const metrics: PoolMetrics = {
      hits: 0,
      misses: 1,
      totalAcquireTime: 0,
      acquireCount: 0,
      avgAcquireTime: 0,
      hitRate: 0,
    };

    // Store pool, config, and metadata
    this.pools.set(connectionString, {
      pool,
      config: poolConfig,
      metrics,
      lastAccessedAt: Date.now(),
    });

    return pool;
  }

  /**
   * Track acquire time for a pool
   * @param connectionString PostgreSQL connection string
   * @param acquireTimeMs Time taken to acquire a connection in milliseconds
   */
  recordAcquireTime(connectionString: string, acquireTimeMs: number): void {
    const entry = this.pools.get(connectionString);
    if (entry) {
      entry.metrics.totalAcquireTime += acquireTimeMs;
      entry.metrics.acquireCount++;
      entry.metrics.avgAcquireTime = entry.metrics.totalAcquireTime / entry.metrics.acquireCount;
    }
  }

  /**
   * Release a connection pool
   * @param connectionString PostgreSQL connection string
   */
  async releasePool(connectionString: string): Promise<void> {
    const entry = this.pools.get(connectionString);
    if (entry) {
      const sanitizedConnStr = sanitizeConnectionString(connectionString);
      logger.info('Releasing connection pool', { connectionString: sanitizedConnStr });
      try {
        // Check if there are active connections before releasing
        const activeCount = entry.pool.totalCount - entry.pool.idleCount;
        if (activeCount > 0) {
          logger.warn('Releasing pool with active connections', {
            connectionString: sanitizedConnStr,
            activeCount,
          });
        }
        await entry.pool.end();
      } catch (error) {
        logger.error('Error releasing pool', {
          connectionString: sanitizedConnStr,
          error: sanitizeErrorMessage(error),
        });
      }
      this.pools.delete(connectionString);
    }
  }

  /**
   * Close all connection pools
   */
  async closeAll(): Promise<void> {
    logger.info('Closing all connection pools', { count: this.pools.size });

    // Clear the cleanup timer
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }

    const promises = Array.from(this.pools.entries()).map(async ([connectionString, entry]) => {
      const sanitizedConnStr = sanitizeConnectionString(connectionString);
      try {
        await entry.pool.end();
        logger.debug('Pool closed', { connectionString: sanitizedConnStr });
      } catch (error) {
        logger.error('Error closing pool', {
          connectionString: sanitizedConnStr,
          error: sanitizeErrorMessage(error),
        });
      }
    });

    await Promise.all(promises);
    this.pools.clear();
  }

  /**
   * Get pool statistics for all pools
   * @returns Map of connection string to pool stats
   */
  getPoolStats(): Map<string, PoolStats> {
    const stats = new Map<string, PoolStats>();

    for (const [connectionString, entry] of this.pools.entries()) {
      stats.set(connectionString, {
        totalCount: entry.pool.totalCount,
        idleCount: entry.pool.idleCount,
        waitingCount: entry.pool.waitingCount,
        max: entry.pool.options.max || 0,
        metrics: { ...entry.metrics },
      });
    }

    return stats;
  }

  /**
   * Get pool statistics for a specific connection string
   * @param connectionString PostgreSQL connection string
   * @returns Pool statistics or null if pool doesn't exist
   */
  getPoolStatsFor(connectionString: string): PoolStats | null {
    const entry = this.pools.get(connectionString);
    if (!entry) {
      return null;
    }

    return {
      totalCount: entry.pool.totalCount,
      idleCount: entry.pool.idleCount,
      waitingCount: entry.pool.waitingCount,
      max: entry.pool.options.max || 0,
      metrics: { ...entry.metrics },
    };
  }

  /**
   * Evict the least recently used pool when the pool limit is reached
   */
  private evictLRUPool(): void {
    let lruKey: string | null = null;
    let lruTime = Infinity;

    for (const [key, entry] of this.pools.entries()) {
      if (entry.lastAccessedAt < lruTime) {
        lruTime = entry.lastAccessedAt;
        lruKey = key;
      }
    }

    if (lruKey) {
      const entry = this.pools.get(lruKey)!;
      // End the pool without awaiting (fire-and-forget) to avoid blocking the caller
      entry.pool.end().catch((err) => {
        logger.error('Error evicting LRU pool', {
          connectionString: sanitizeConnectionString(lruKey!),
          error: sanitizeErrorMessage(err),
        });
      });
      this.pools.delete(lruKey);
    }
  }

  /**
   * Clean up idle pools that have no active connections
   */
  private async cleanupIdlePools(): Promise<void> {
    const now = Date.now();
    const cleanupPromises: Promise<void>[] = [];

    for (const [connectionString, entry] of this.pools.entries()) {
      // Release pools that have been idle (no active connections) for more than the cleanup interval
      const idleDuration = now - entry.lastAccessedAt;
      const totalCount = entry.pool.totalCount;

      if (totalCount === 0 && idleDuration > this.idlePoolCleanupMs) {
        const sanitizedConnStr = sanitizeConnectionString(connectionString);
        logger.debug('Cleaning up idle pool', { connectionString: sanitizedConnStr, idleDuration });
        cleanupPromises.push(
          entry.pool.end().catch((err) => {
            logger.error('Error cleaning up idle pool', {
              connectionString: sanitizedConnStr,
              error: sanitizeErrorMessage(err),
            });
          }).then(() => {
            this.pools.delete(connectionString);
          })
        );
      }
    }

    await Promise.all(cleanupPromises);
  }
}

/**
 * Sanitize a PostgreSQL connection string by masking the password
 * @param connStr PostgreSQL connection string
 * @returns Sanitized connection string with password masked
 */
function sanitizeConnectionString(connStr: string): string {
  try {
    // Mask password in format: postgresql://user:password@host/db
    return connStr.replace(/:[^:@]*@/, ':***@');
  } catch {
    return '[invalid connection string]';
  }
}

/**
 * Safely extract error message from an unknown error value
 */
function sanitizeErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

/**
 * Global connection pool instance
 */
let globalPool: PostgresConnectionPool | null = null;

/**
 * Get the global connection pool instance
 * @returns Global PostgresConnectionPool instance
 */
export function getGlobalConnectionPool(): PostgresConnectionPool {
  if (!globalPool) {
    globalPool = new PostgresConnectionPool();
    logger.debug('Created global connection pool');
  }
  return globalPool;
}

/**
 * Reset the global connection pool (useful for testing)
 */
export async function resetGlobalConnectionPool(): Promise<void> {
  if (globalPool) {
    await globalPool.closeAll();
    globalPool = null;
    logger.debug('Reset global connection pool');
  }
}