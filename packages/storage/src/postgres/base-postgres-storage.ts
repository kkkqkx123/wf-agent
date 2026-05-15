/**
 * PostgreSQL Storage Base Class
 * Provides general database storage functionality, including connection management, error handling, initialization, etc.
 */

import { Pool, PoolClient, DatabaseError } from 'pg';
import { StorageError, StorageInitializationError } from '../types/storage-errors.js';
import { createModuleLogger } from '../logger.js';
import {
  PostgresConnectionPool,
  getGlobalConnectionPool,
} from './connection-pool.js';
import type { StorageMetrics } from '../types/metrics.js';
import { DEFAULT_STORAGE_METRICS } from '../types/metrics.js';

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
  
  /** Schema version for migration support (default: 1) */
  schemaVersion?: number;
}

/**
 * PostgreSQL File Storage Abstract Base Class
 * @template TMetadata metadata type
 */
export abstract class BasePostgresStorage<TMetadataType> {
  protected pool: Pool | null = null;
  protected initialized: boolean = false;
  protected usingPool: boolean = false;
  private connectionPool: PostgresConnectionPool | null = null;
  protected metrics: StorageMetrics = { ...DEFAULT_STORAGE_METRICS };
  protected loadCounter: number = 0; // Counter for integrity check frequency

  constructor(protected readonly config: BasePostgresStorageConfig) {}

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
  async initialize(): Promise<void> {
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

      logger.info('PostgreSQL storage initialized', {
        connectionString: this.config.connectionString,
        tableName: this.getTableName(),
        usingPool: this.usingPool,
        schemaVersion: this.getCurrentSchemaVersion(),
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
   * Initialize or migrate database schema
   */
  private async initializeSchema(): Promise<void> {
    const client = await this.getClient();
    
    try {
      const targetVersion = this.config.schemaVersion ?? 1;

      // Create schema version tracking table if it doesn't exist
      await client.query(`
        CREATE TABLE IF NOT EXISTS _schema_versions (
          table_name TEXT PRIMARY KEY,
          version INTEGER NOT NULL,
          updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
      `);

      // Get current version for this table
      const result = await client.query(
        'SELECT version FROM _schema_versions WHERE table_name = $1',
        [this.getTableName()]
      );

      const installedVersion = result.rows.length > 0 ? result.rows[0].version : 0;

      if (installedVersion === 0) {
        // Fresh installation - create tables
        logger.info('Creating new schema', { 
          table: this.getTableName(), 
          version: targetVersion 
        });
        await this.createTableSchema(client);
        
        // Record version
        await client.query(
          'INSERT INTO _schema_versions (table_name, version, updated_at) VALUES ($1, $2, NOW())',
          [this.getTableName(), targetVersion]
        );
      } else if (installedVersion < targetVersion) {
        // Migration needed
        logger.info('Migrating schema', { 
          table: this.getTableName(), 
          fromVersion: installedVersion, 
          toVersion: targetVersion 
        });
        await this.migrateSchema(client, installedVersion, targetVersion);
        
        // Update version
        await client.query(
          'UPDATE _schema_versions SET version = $1, updated_at = NOW() WHERE table_name = $2',
          [targetVersion, this.getTableName()]
        );
      } else if (installedVersion > targetVersion) {
        logger.warn('Database schema version is newer than expected', {
          table: this.getTableName(),
          installedVersion,
          targetVersion,
        });
      } else {
        logger.debug('Schema is up to date', { 
          table: this.getTableName(), 
          version: installedVersion 
        });
      }
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get current schema version
   */
  protected getCurrentSchemaVersion(): number {
    return this.config.schemaVersion ?? 1;
  }

  /**
   * Migrate schema from one version to another
   * Override this method in subclasses to implement custom migrations
   * @param client Database client
   * @param fromVersion Current schema version
   * @param toVersion Target schema version
   */
  protected async migrateSchema(
    _client: PoolClient,
    fromVersion: number,
    toVersion: number
  ): Promise<void> {
    // Default implementation does nothing
    // Subclasses should override to implement actual migrations
    logger.warn('Schema migration not implemented', {
      table: this.getTableName(),
      fromVersion,
      toVersion,
    });
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
   * Ensure that it has been initialized
   */
  protected ensureInitialized(): void {
    if (!this.initialized || !this.pool) {
      throw new StorageError('Storage not initialized. Call initialize() first.', 'initialize');
    }
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
   * Close the storage connection
   * If using connection pool, releases the pool instead of closing it
   */
  async close(): Promise<void> {
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
   * Get storage metrics
   */
  async getMetrics(): Promise<StorageMetrics> {
    try {
      const client = await this.getClient();
      
      try {
        // Query database for accurate size and count information
        const tableName = this.getTableName();
        const result = await client.query(
          `SELECT COUNT(*) as count, COALESCE(SUM(blob_size), 0) as total_blob_size FROM ${tableName}`
        );

        const sizeInfo = result.rows[0];
        return {
          ...this.metrics,
          totalCount: parseInt(sizeInfo.count) || 0,
          totalBlobSize: parseInt(sizeInfo.total_blob_size) || 0,
        };
      } finally {
        this.releaseClient(client);
      }
    } catch (error) {
      logger.error('Failed to get metrics', { error });
      return { ...this.metrics };
    }
  }

  /**
   * Reset metrics counters
   */
  resetMetrics(): void {
    this.metrics = {
      saveCount: 0,
      loadCount: 0,
      deleteCount: 0,
      listCount: 0,
      avgSaveTime: 0,
      avgLoadTime: 0,
      avgDeleteTime: 0,
      avgListTime: 0,
      totalMetadataSize: this.metrics.totalMetadataSize,
      totalBlobSize: this.metrics.totalBlobSize,
      totalCount: this.metrics.totalCount,
    };
  }

  /**
   * Clear all data
   */
  async clear(): Promise<void> {
    const client = await this.getClient();
    
    try {
      await client.query(`DELETE FROM ${this.getTableName()}`);
      
      const blobTableName = this.getBlobTableName();
      if (blobTableName) {
        await client.query(`DELETE FROM ${blobTableName}`);
      }
      
      logger.info('PostgreSQL tables cleared', { 
        table: this.getTableName(),
        blobTable: blobTableName 
      });
    } catch (error) {
      return this.handlePostgresError(error, 'clear', {});
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
      return this.handlePostgresError(error, 'optimize', {});
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save multiple items in a single transaction
   */
  async saveBatch(
    items: Array<{ id: string; data: Uint8Array; metadata: TMetadataType }>,
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
      return this.handlePostgresError(error, 'saveBatch', { count: items.length });
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
    metadata: TMetadataType
  ): Promise<void>;

  /**
   * Load multiple items efficiently
   */
  async loadBatch(
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
      return this.handlePostgresError(error, 'loadBatch', { count: ids.length });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete multiple items in a single transaction
   */
  async deleteBatch(ids: string[]): Promise<void> {
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
      return this.handlePostgresError(error, 'deleteBatch', { count: ids.length });
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
