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
}

/**
 * Pool statistics
 */
export interface PoolStats {
  totalCount: number;
  idleCount: number;
  waitingCount: number;
  max: number;
}

/**
 * PostgreSQL Connection Pool
 * Manages connection pools per database connection string
 */
export class PostgresConnectionPool {
  private pools: Map<string, Pool> = new Map();
  private poolConfigs: Map<string, PostgresPoolConfig> = new Map();

  /**
   * Get or create a connection pool for the given connection string
   * @param connectionString PostgreSQL connection string
   * @param config Optional pool configuration
   * @returns Pool instance
   */
  getPool(connectionString: string, config?: PostgresPoolConfig): Pool {
    // Check if pool already exists
    const existingPool = this.pools.get(connectionString);
    if (existingPool) {
      logger.debug('Reusing existing connection pool', { connectionString });
      return existingPool;
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
      connectionString,
      config: {
        max: poolConfig.max,
        min: poolConfig.min,
        idleTimeoutMillis: poolConfig.idleTimeoutMillis,
      },
    });

    const pool = new Pool({
      connectionString,
      max: poolConfig.max,
      min: poolConfig.min,
      idleTimeoutMillis: poolConfig.idleTimeoutMillis,
      connectionTimeoutMillis: poolConfig.connectionTimeoutMillis,
      maxUses: poolConfig.maxUses,
    });

    // Set up event listeners for monitoring
    pool.on('error', (err) => {
      logger.error('Unexpected pool error', {
        connectionString,
        error: err.message,
      });
    });

    pool.on('connect', () => {
      logger.debug('New client connected to pool', { connectionString });
    });

    pool.on('acquire', () => {
      logger.debug('Client acquired from pool', { connectionString });
    });

    pool.on('remove', () => {
      logger.debug('Client removed from pool', { connectionString });
    });

    // Store pool and config
    this.pools.set(connectionString, pool);
    this.poolConfigs.set(connectionString, poolConfig);

    return pool;
  }

  /**
   * Release a connection pool
   * @param connectionString PostgreSQL connection string
   */
  async releasePool(connectionString: string): Promise<void> {
    const pool = this.pools.get(connectionString);
    if (pool) {
      logger.info('Releasing connection pool', { connectionString });
      await pool.end();
      this.pools.delete(connectionString);
      this.poolConfigs.delete(connectionString);
    }
  }

  /**
   * Close all connection pools
   */
  async closeAll(): Promise<void> {
    logger.info('Closing all connection pools', { count: this.pools.size });
    
    const promises = Array.from(this.pools.entries()).map(async ([connectionString, pool]) => {
      try {
        await pool.end();
        logger.debug('Pool closed', { connectionString });
      } catch (error) {
        logger.error('Error closing pool', {
          connectionString,
          error: (error as Error).message,
        });
      }
    });

    await Promise.all(promises);
    this.pools.clear();
    this.poolConfigs.clear();
  }

  /**
   * Get pool statistics for all pools
   * @returns Map of connection string to pool stats
   */
  getPoolStats(): Map<string, PoolStats> {
    const stats = new Map<string, PoolStats>();
    
    for (const [connectionString, pool] of this.pools.entries()) {
      stats.set(connectionString, {
        totalCount: pool.totalCount,
        idleCount: pool.idleCount,
        waitingCount: pool.waitingCount,
        max: pool.options.max || 0,
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
    const pool = this.pools.get(connectionString);
    if (!pool) {
      return null;
    }

    return {
      totalCount: pool.totalCount,
      idleCount: pool.idleCount,
      waitingCount: pool.waitingCount,
      max: pool.options.max || 0,
    };
  }
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
