/**
 * PostgreSQL Storage Base Class
 * Provides general database storage functionality, including connection management, error handling, initialization, etc.
 */

import { Pool, PoolClient, DatabaseError } from 'pg';
import { StorageError, StorageInitializationError } from '../types/storage-errors.js';
import { StorageAdapterBase } from '../types/adapter/storage-adapter-base.js';
import { createModuleLogger } from '../logger.js';
import {
  PostgresConnectionPool,
  getGlobalConnectionPool,
} from './connection-pool.js';
import type { StorageMetrics } from '../types/metrics.js';

const logger = createModuleLogger('postgres-storage');

/**
 * Default pagination limits
 */
const DEFAULT_PAGE_LIMIT = 100;
const MAX_PAGE_LIMIT = 1000;

/**
 * PostgreSQL Storage Configuration
 */
export interface BasePostgresStorageConfig {
  /** PostgreSQL connection string */
  connectionString: string;
  
  /** Connection pool configuration */
  poolConfig?: {
    max?: number;
    min?: number;
    idleTimeoutMillis?: number;
    connectionTimeoutMillis?: number;
    maxUses?: number;
  };
  
  /** Whether to enable logging */
  enableLogging?: boolean;
  
  /** Use shared connection pool (default: true) */
  useConnectionPool?: boolean;
  
  /** Custom connection pool instance (optional, uses global pool if not provided) */
  connectionPool?: PostgresConnectionPool;
  
  /** Enable data integrity verification on load (default: false for performance) */
  verifyIntegrity?: boolean;

  /** Verify integrity every Nth load operation (default: 100, only used when verifyIntegrity is true) */
  integrityCheckFrequency?: number;

  /** Auto-maintenance interval in milliseconds (default: not set, disabled).
   * When set, runs VACUUM ANALYZE periodically to prevent performance degradation. */
  maintenanceIntervalMs?: number;
}

/**
 * PostgreSQL File Storage Abstract Base Class
 * @template TMetadata metadata type
 * @template TListOptions list options type
 */
export abstract class BasePostgresStorage<TMetadata, TListOptions = Record<string, unknown>, TSaveOptions = void>
  extends StorageAdapterBase<TMetadata, TListOptions, TSaveOptions>
{
  protected pool: Pool | null = null;
  protected usingPool: boolean = false;
  private connectionPool: PostgresConnectionPool | null = null;
  protected loadCounter: number = 0; // Counter for integrity check frequency
  private maintenanceTimer: ReturnType<typeof setInterval> | null = null;

  constructor(protected readonly config: BasePostgresStorageConfig) {
    super();
  }

  /**
   * Get table name
   * Subclasses must implement this method to return the table name.
   */
  protected abstract getTableName(): string;

  /**
   * Get BLOB table name (return null if not using separate BLOB table)
   * Subclasses must implement this method
   */
  protected abstract getBlobTableName(): string | null;

  /**
   * Creating a Table Structure
   * Subclasses must implement this method to create a concrete table structure
   */
  protected abstract createTableSchema(client: PoolClient): Promise<void>;

  /**
   * Initializing Storage
   * Creating database connections and table structures
   */
  override async initialize(): Promise<void> {
    logger.debug('Initializing PostgreSQL storage', {
      connectionString: this.config.connectionString,
      usePool: this.config.useConnectionPool ?? true,
    });

    try {
      const usePool = this.config.useConnectionPool ?? true;

      if (usePool) {
        // Use connection pool
        this.connectionPool = this.config.connectionPool ?? getGlobalConnectionPool();
        this.pool = this.connectionPool.getPool(
          this.config.connectionString,
          this.config.poolConfig
        );
        this.usingPool = true;
        logger.debug('Using pooled PostgreSQL connection', {
          connectionString: this.config.connectionString,
        });
      } else {
        // Create dedicated pool with single connection
        this.pool = new Pool({
          connectionString: this.config.connectionString,
          max: 1,
          min: 1,
          ...this.config.poolConfig,
        });
        this.usingPool = false;
        logger.debug('Created dedicated PostgreSQL connection', {
          connectionString: this.config.connectionString,
        });
      }

      // Mark as initialized so that createTableSchema can use getClient
      this.initialized = true;

      // Create or migrate schema
      await this.initializeSchema();

      // Start auto-maintenance timer if interval is configured
      if (this.config.maintenanceIntervalMs && this.config.maintenanceIntervalMs > 0) {
        this.startMaintenanceTimer(this.config.maintenanceIntervalMs);
      }

      logger.info('PostgreSQL storage initialized', {
        connectionString: this.config.connectionString,
        tableName: this.getTableName(),
        usingPool: this.usingPool,
      });
    } catch (error) {
      this.initialized = false;
      
      // Clean up pool on failure to prevent resource leaks
      if (this.pool) {
        try {
          if (this.usingPool && this.connectionPool) {
            // Don't end pooled connections, just release reference
            this.pool = null;
          } else {
            await this.pool.end();
          }
        } catch (cleanupError) {
          logger.error('Error cleaning up pool after initialization failure', {
            connectionString: this.config.connectionString,
            error: (cleanupError as Error).message,
          });
        } finally {
          this.pool = null;
        }
      }
      
      logger.error('Failed to initialize PostgreSQL storage', {
        connectionString: this.config.connectionString,
        error: (error as Error).message,
      });
      throw new StorageInitializationError(
        `Failed to initialize PostgreSQL storage: ${this.config.connectionString}`,
        error as Error,
      );
    }
  }

  /**
   * Initialize database schema
   */
  private async initializeSchema(): Promise<void> {
    const client = await this.getClient();
    
    try {
      // Create tables (IF NOT EXISTS ensures idempotency)
      logger.info('Initializing schema', { table: this.getTableName() });
      await this.createTableSchema(client);
      
      logger.debug('Schema initialized', { table: this.getTableName() });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Validate and normalize pagination parameters
   */
  protected validatePagination(limit?: number, offset?: number): { limit: number; offset: number } {
    const validatedLimit = limit !== undefined 
      ? Math.min(Math.max(1, limit), MAX_PAGE_LIMIT) 
      : DEFAULT_PAGE_LIMIT;
    const validatedOffset = offset !== undefined ? Math.max(0, offset) : 0;
    
    return { limit: validatedLimit, offset: validatedOffset };
  }

  /**
   * Get a client from the pool
   */
  protected async getClient(): Promise<PoolClient> {
    this.ensureInitialized();
    return this.pool!.connect();
  }

  /**
   * Release a client back to the pool
   */
  protected releaseClient(client: PoolClient): void {
    client.release();
  }

  /**
   * Compute hash for BLOB data using Web Crypto API
   * Uses sampling for large objects to improve performance
   */
  protected async computeHash(data: Uint8Array): Promise<string> {
    // For very large objects (>1MB), use sampling to improve performance
    const LARGE_OBJECT_THRESHOLD = 1024 * 1024; // 1MB
    
    let hashInput: Uint8Array;
    if (data.length > LARGE_OBJECT_THRESHOLD) {
      // Sample-based hashing: first 64KB + last 64KB + size
      const sampleSize = 64 * 1024; // 64KB
      const sample = new Uint8Array(sampleSize * 2 + 8);
      
      // Copy first 64KB
      sample.set(data.slice(0, sampleSize));
      // Copy last 64KB
      sample.set(data.slice(-sampleSize), sampleSize);
      // Append size as uint64
      const view = new DataView(sample.buffer);
      view.setBigUint64(sampleSize * 2, BigInt(data.length), true); // little-endian
      
      hashInput = sample;
    } else {
      hashInput = data;
    }
    
    // Convert to ArrayBuffer for crypto API compatibility
    const arrayBuffer = hashInput.buffer.slice(
      hashInput.byteOffset, 
      hashInput.byteOffset + hashInput.byteLength
    ) as ArrayBuffer;
    
    const hashBuffer = await crypto.subtle.digest('SHA-256', arrayBuffer);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('').substring(0, 16);
  }

  /**
   * Verify data integrity by comparing hash
   */
  protected async verifyIntegrity(
    data: Uint8Array,
    expectedHash: string,
    id: string,
  ): Promise<void> {
    const actualHash = await this.computeHash(data);
    if (actualHash !== expectedHash) {
      logger.error('Data integrity check failed', { 
        id, 
        expected: expectedHash, 
        actual: actualHash 
      });
      throw new StorageError(
        'Data integrity verification failed',
        'load',
        { id, expectedHash, actualHash }
      );
    }
  }

  /**
   * Check if integrity verification should be performed
   */
  protected shouldVerifyIntegrity(): boolean {
    if (!this.config.verifyIntegrity) {
      return false;
    }
    
    const frequency = this.config.integrityCheckFrequency ?? 100;
    this.loadCounter++;
    return this.loadCounter % frequency === 0;
  }

  /**
   * Handling PostgreSQL Errors
   */
  protected handlePostgresError(
    error: unknown,
    operation: string,
    context?: Record<string, unknown>,
  ): never {
    logger.error('PostgreSQL operation failed', {
      operation,
      context,
      error: (error as Error).message,
    });

    if (error instanceof DatabaseError) {
      throw new StorageError(
        `PostgreSQL error [${error.code}]: ${error.message}`,
        operation,
        { ...context, code: error.code },
        error,
      );
    }

    throw new StorageError(
      `Storage operation failed: ${operation}`,
      operation,
      context,
      error as Error,
    );
  }

  /**
   * Start periodic maintenance timer
   * Runs VACUUM ANALYZE periodically to prevent performance degradation
   */
  private startMaintenanceTimer(intervalMs: number): void {
    if (this.maintenanceTimer) {
      clearInterval(this.maintenanceTimer);
    }

    this.maintenanceTimer = setInterval(async () => {
      try {
        logger.info('Running periodic maintenance', { table: this.getTableName() });
        await this.optimize();
      } catch (error) {
        logger.warn('Periodic maintenance task failed', {
          table: this.getTableName(),
          error: (error as Error).message,
        });
      }
    }, intervalMs);

    logger.debug('Periodic maintenance timer started', {
      table: this.getTableName(),
      intervalMs,
    });
  }

  /**
   * Close the storage connection
   * If using connection pool, releases the pool instead of closing it
   */
  override async close(): Promise<void> {
    // Clear maintenance timer
    if (this.maintenanceTimer) {
      clearInterval(this.maintenanceTimer);
      this.maintenanceTimer = null;
    }

    if (this.pool) {
      try {
        if (this.usingPool && this.connectionPool) {
          // Release pool back to connection pool manager
          await this.connectionPool.releasePool(this.config.connectionString);
          logger.info('PostgreSQL pool released to pool manager', {
            connectionString: this.config.connectionString,
          });
        } else {
          // End dedicated pool
          await this.pool.end();
          logger.info('PostgreSQL storage closed', {
            connectionString: this.config.connectionString,
          });
        }
      } catch (error) {
        logger.error('Error closing PostgreSQL database', {
          connectionString: this.config.connectionString,
          error: (error as Error).message,
        });
      } finally {
        this.pool = null;
        this.initialized = false;
      }
    }
  }

  /**
   * Get storage metrics
   */
  override async getMetrics(): Promise<StorageMetrics> {
    try {
      const client = await this.getClient();
      
      try {
        // Query database for accurate size and count information
        const tableName = this.getTableName();
        const result = await client.query(
          `SELECT COUNT(*) as count, COALESCE(SUM(blob_size), 0) as total_blob_size FROM ${tableName}`
        );

        const sizeInfo = result.rows[0];
        return this.populateCacheMetrics({
          ...this.metrics,
          totalCount: parseInt(sizeInfo.count) || 0,
          totalBlobSize: parseInt(sizeInfo.total_blob_size) || 0,
        });
      } finally {
        this.releaseClient(client);
      }
    } catch (error) {
      logger.error('Failed to get metrics', { error });
      return this.populateCacheMetrics({ ...this.metrics });
    }
  }

  // ── Template methods for cache integration ─────────────────────────────
  /**
   * Subclasses must implement doSave() instead of save().
   * The base class wraps doSave() with cache invalidation.
   * @param options - Optional save options (TSaveOptions), forwarded from save()
   */
  protected abstract doSave(id: string, data: Uint8Array, metadata: TMetadata, options?: TSaveOptions): Promise<void>;

  /**
   * Subclasses must implement doLoad() instead of load().
   * The base class wraps doLoad() with read-through caching.
   */
  protected abstract doLoad(id: string): Promise<Uint8Array | null>;

  override async save(id: string, data: Uint8Array, metadata: TMetadata, options?: TSaveOptions): Promise<void> {
    await this.saveAndInvalidateCache(id, () => this.doSave(id, data, metadata, options));
  }

  override async load(id: string): Promise<Uint8Array | null> {
    return this.loadFromCache(id, () => this.doLoad(id));
  }

  /**
   * Subclasses must implement doDelete() instead of delete().
   * The base class wraps doDelete() with cache invalidation.
   */
  protected abstract doDelete(id: string): Promise<void>;

  override async delete(id: string): Promise<void> {
    await this.deleteAndInvalidateCache(id, () => this.doDelete(id));
  }

  // ── CRUD abstract methods (must be implemented by subclasses) ──────────
  abstract override list(options?: TListOptions): Promise<string[]>;
  abstract override getMetadata(id: string): Promise<TMetadata | null>;

  /**
   * Clear all data
   */
  override async clear(): Promise<void> {
    const client = await this.getClient();
    
    try {
      await client.query(`DELETE FROM ${this.getTableName()}`);
      
      const blobTableName = this.getBlobTableName();
      if (blobTableName) {
        await client.query(`DELETE FROM ${blobTableName}`);
      }
      
      this.clearCache();
      logger.info('PostgreSQL tables cleared', { 
        table: this.getTableName(),
        blobTable: blobTableName 
      });
    } catch (error) {
      this.handlePostgresError(error, 'clear', {});
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Optimize database by running VACUUM and ANALYZE
   */
  async optimize(): Promise<void> {
    const client = await this.getClient();

    try {
      logger.info('Starting database optimization', { table: this.getTableName() });
      
      // Reclaim unused space
      await client.query('VACUUM ANALYZE');
      
      logger.info('Database optimization completed', { table: this.getTableName() });
    } catch (error) {
      this.handlePostgresError(error, 'optimize', {});
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save multiple items in a single transaction
   */
  override async saveBatch(
    items: Array<{ id: string; data: Uint8Array; metadata: TMetadata }>,
  ): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      if (items.length === 0) {
        return;
      }

      // Begin transaction
      await client.query('BEGIN');

      try {
        for (const item of items) {
          await this.saveToClient(client, item.id, item.data, item.metadata);
        }
        
        // Commit transaction
        await client.query('COMMIT');

        const elapsed = Date.now() - startTime;
        this.updateMetric('save', elapsed / items.length, items.reduce((sum, item) => sum + item.data.length, 0));

        logger.debug('Batch save completed', {
          table: this.getTableName(),
          count: items.length,
          totalTimeMs: elapsed,
        });
      } catch (error) {
        // Rollback on error
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      this.handlePostgresError(error, 'saveBatch', { count: items.length });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save to client (to be implemented by subclasses)
   */
  protected abstract saveToClient(
    client: PoolClient,
    id: string,
    data: Uint8Array,
    metadata: TMetadata
  ): Promise<void>;

  /**
   * Load multiple items efficiently
   */
  override async loadBatch(
    ids: string[],
  ): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      if (ids.length === 0) {
        return [];
      }

      // Use ANY clause for efficient batch loading
      const result = await client.query(
        `SELECT id, blob_data FROM ${this.getBlobTableName() || this.getTableName()} WHERE id = ANY($1)`,
        [ids]
      );

      // Create a map for quick lookup
      const dataMap = new Map<string, Uint8Array>();
      for (const row of result.rows) {
        dataMap.set(row.id, row.blob_data);
      }

      // Maintain order and handle missing items
      const results = ids.map(id => ({
        id,
        data: dataMap.get(id) || null,
      }));

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed / ids.length);

      logger.debug('Batch load completed', {
        table: this.getTableName(),
        requested: ids.length,
        found: result.rows.length,
        totalTimeMs: elapsed,
      });

      return results;
    } catch (error) {
      this.handlePostgresError(error, 'loadBatch', { count: ids.length });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete multiple items in a single transaction
   */
  override async deleteBatch(ids: string[]): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      if (ids.length === 0) {
        return;
      }

      // Begin transaction
      await client.query('BEGIN');

      try {
        for (const id of ids) {
          await this.deleteFromClient(client, id);
        }
        
        // Commit transaction
        await client.query('COMMIT');

        // Invalidate cache for all deleted ids
        for (const id of ids) {
          this.cache?.delete(id);
        }

        const elapsed = Date.now() - startTime;
        this.updateMetric('delete', elapsed / ids.length);

        logger.debug('Batch delete completed', {
          table: this.getTableName(),
          count: ids.length,
          totalTimeMs: elapsed,
        });
      } catch (error) {
        // Rollback on error
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      this.handlePostgresError(error, 'deleteBatch', { count: ids.length });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete from client (to be implemented by subclasses)
   */
  protected abstract deleteFromClient(client: PoolClient, id: string): Promise<void>;

  /**
   * Get connection pool metrics (PostgreSQL only)
   * Returns null if not using connection pool
   * 
   * @returns Pool metrics or null if pool is not available
   * 
   * @example
   * ```typescript
   * const metrics = await storage.getPoolMetrics();
   * if (metrics && metrics.utilization > 0.8) {
   *   logger.warn('Connection pool utilization high', metrics);
   * }
   * ```
   */
  async getPoolMetrics(): Promise<{ activeConnections: number; idleConnections: number; waitingRequests: number; maxConnections: number; utilization: number } | null> {
    if (!this.usingPool || !this.connectionPool) {
      return null;
    }
    
    const stats = this.connectionPool.getPoolStatsFor(this.config.connectionString);
    if (!stats) {
      return null;
    }
    
    const active = stats.totalCount - stats.idleCount;
    
    return {
      activeConnections: active,
      idleConnections: stats.idleCount,
      waitingRequests: stats.waitingCount,
      maxConnections: stats.max,
      utilization: stats.max > 0 ? active / stats.max : 0,
    };
  }
}
