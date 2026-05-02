/**
 * SQLite Storage Base Class
 * Provides general database storage functionality, including connection management, error handling, initialization, etc.
 */

import Database, { SqliteError } from "better-sqlite3";
import { StorageError, StorageInitializationError } from "../types/storage-errors.js";
import { createModuleLogger } from "../logger.js";
import {
  SqliteConnectionPool,
  getGlobalConnectionPool,
} from "./connection-pool.js";
import type { StorageMetrics } from "../types/metrics.js";
import { DEFAULT_STORAGE_METRICS } from "../types/metrics.js";

const logger = createModuleLogger("sqlite-storage");

/**
 * SQLite Storage Basics Configuration
 */
export interface BaseSqliteStorageConfig {
  /** Database File Path */
  dbPath: string;
  /** Whether to enable logging */
  enableLogging?: boolean;
  /** Whether to open in read-only mode */
  readonly?: boolean;
  /** Whether to throw an error if the database file does not exist */
  fileMustExist?: boolean;
  /** Timeout in milliseconds for database locks */
  timeout?: number;
  /** Use shared connection pool (default: true) */
  useConnectionPool?: boolean;
  /** Custom connection pool instance (optional, uses global pool if not provided) */
  connectionPool?: SqliteConnectionPool;
}

/**
 * SQLite File Storage Abstract Base Class
 * @template TMetadata metadata type
 */
export abstract class BaseSqliteStorage<_TMetadataType> {
  protected db: Database.Database | null = null;
  protected initialized: boolean = false;
  protected usingPool: boolean = false;
  private connectionPool: SqliteConnectionPool | null = null;
  protected metrics: StorageMetrics = { ...DEFAULT_STORAGE_METRICS };

  constructor(protected readonly config: BaseSqliteStorageConfig) {}

  /**
   * Get table name
   * Subclasses must implement this method to return the table name.
   */
  protected abstract getTableName(): string;

  /**
   * Creating a Table Structure
   * Subclasses must implement this method to create a concrete table structure
   */
  protected abstract createTableSchema(): void;

  /**
   * Initializing Storage
   * Creating database connections and table structures
   */
  async initialize(): Promise<void> {
    logger.debug("Initializing SQLite storage", {
      dbPath: this.config.dbPath,
      readonly: this.config.readonly,
      usePool: this.config.useConnectionPool ?? true,
    });

    try {
      const usePool = this.config.useConnectionPool ?? true;

      if (usePool && !this.config.readonly) {
        // Use connection pool
        this.connectionPool = this.config.connectionPool ?? getGlobalConnectionPool();
        this.db = this.connectionPool.getConnection(this.config.dbPath);
        this.usingPool = true;
        logger.debug("Using pooled SQLite connection", { dbPath: this.config.dbPath });
      } else {
        // Create dedicated connection (for readonly or when pool is disabled)
        const options: Database.Options = {
          readonly: this.config.readonly ?? false,
          fileMustExist: this.config.fileMustExist ?? false,
          timeout: this.config.timeout ?? 5000,
        };

        this.db = new Database(this.config.dbPath, options);
        this.usingPool = false;

        // Enable WAL mode to improve concurrency performance
        this.db.pragma("journal_mode = WAL");
        this.db.pragma("wal_autocheckpoint = 1000");
        this.db.pragma("synchronous = NORMAL");

        logger.debug("Created dedicated SQLite connection", { dbPath: this.config.dbPath });
      }

      // is marked as initialized so that createTableSchema can use the getDb
      this.initialized = true;

      // Create table structure (skip in read-only mode)
      if (!this.config.readonly) {
        this.createTableSchema();
      }

      logger.info("SQLite storage initialized", {
        dbPath: this.config.dbPath,
        tableName: this.getTableName(),
        usingPool: this.usingPool,
      });
    } catch (error) {
      this.initialized = false;
      logger.error("Failed to initialize SQLite storage", {
        dbPath: this.config.dbPath,
        error: (error as Error).message,
      });
      throw new StorageInitializationError(
        `Failed to initialize SQLite storage: ${this.config.dbPath}`,
        error as Error,
      );
    }
  }

  /**
   * Ensure that it has been initialized
   */
  protected ensureInitialized(): void {
    if (!this.initialized || !this.db) {
      throw new StorageError("Storage not initialized. Call initialize() first.", "initialize");
    }
  }

  /**
   * Getting a database instance
   */
  protected getDb(): Database.Database {
    this.ensureInitialized();
    return this.db!;
  }

  /**
   * Handling SQLite Errors
   */
  protected handleSqliteError(
    error: unknown,
    operation: string,
    context?: Record<string, unknown>,
  ): never {
    logger.error("SQLite operation failed", {
      operation,
      context,
      error: (error as Error).message,
    });

    if (error instanceof SqliteError) {
      throw new StorageError(
        `SQLite error [${error.code}]: ${error.message}`,
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
   * Load data from database
   */
  async load(id: string): Promise<Uint8Array | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT data FROM ${this.getTableName()} WHERE id = ?`);
      const row = stmt.get(id) as { data: Buffer } | undefined;

      if (!row) {
        logger.debug("Data not found in SQLite", { id, table: this.getTableName() });
        return null;
      }

      logger.debug("Data loaded from SQLite", { id, dataSize: row.data.length });
      return new Uint8Array(row.data);
    } catch (error) {
      this.handleSqliteError(error, "load", { id });
    }
  }

  /**
   * Delete data
   */
  async delete(id: string): Promise<void> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`DELETE FROM ${this.getTableName()} WHERE id = ?`);
      stmt.run(id);
      logger.debug("Data deleted from SQLite", { id, table: this.getTableName() });
    } catch (error) {
      this.handleSqliteError(error, "delete", { id });
    }
  }

  /**
   * Check if data exists
   */
  async exists(id: string): Promise<boolean> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT 1 FROM ${this.getTableName()} WHERE id = ?`);
      const row = stmt.get(id);
      return row !== undefined;
    } catch (error) {
      this.handleSqliteError(error, "exists", { id });
    }
  }

  /**
   * Clear all data
   */
  async clear(): Promise<void> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`DELETE FROM ${this.getTableName()}`);
      stmt.run();
      logger.info("SQLite table cleared", { table: this.getTableName() });
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
    }
  }

  /**
   * Close the storage connection
   * If using connection pool, releases the connection instead of closing it
   */
  async close(): Promise<void> {
    if (this.db) {
      try {
        if (this.usingPool && this.connectionPool) {
          // Release connection back to pool
          this.connectionPool.releaseConnection(this.config.dbPath);
          logger.info("SQLite connection released to pool", { dbPath: this.config.dbPath });
        } else {
          // Close dedicated connection
          this.db.close();
          logger.info("SQLite storage closed", { dbPath: this.config.dbPath });
        }
      } catch (error) {
        logger.error("Error closing SQLite database", {
          dbPath: this.config.dbPath,
          error: (error as Error).message,
        });
      } finally {
        this.db = null;
        this.initialized = false;
      }
    }
  }

  /**
   * Update metrics for an operation
   * @param operation Operation name (save, load, delete, list)
   * @param timeMs Time taken in milliseconds
   * @param dataSize Optional data size in bytes
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
   * @returns Storage metrics including operation counts, timings, and sizes
   */
  async getMetrics(): Promise<StorageMetrics> {
    try {
      const db = this.getDb();
      
      // Query database for accurate size and count information
      const tableName = this.getTableName();
      const sizeInfo = db.prepare(
        `SELECT COUNT(*) as count, COALESCE(SUM(blob_size), 0) as total_blob_size FROM ${tableName}`
      ).get() as { count: number; total_blob_size: number };

      return {
        ...this.metrics,
        totalCount: sizeInfo.count,
        totalBlobSize: sizeInfo.total_blob_size || 0,
      };
    } catch (error) {
      logger.error("Failed to get metrics", { error });
      return { ...this.metrics };
    }
  }

  /**
   * Reset metrics counters
   * Preserves size information but resets operation counts and timings
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
   * Save multiple items in a single transaction
   * More efficient than individual saves for bulk operations
   * @param items Array of items to save with id, data, and metadata
   */
  async saveBatch(
    items: Array<{ id: string; data: Uint8Array; metadata: any }>,
  ): Promise<void> {
    const db = this.getDb();
    const startTime = Date.now();

    try {
      // Use transaction for atomicity and performance
      const transaction = db.transaction(() => {
        for (const item of items) {
          // Default implementation: simple save without custom logic
          // Subclasses should override this method for custom save behavior
          const stmt = db.prepare(`
            INSERT INTO ${this.getTableName()} (id, data) VALUES (?, ?)
            ON CONFLICT(id) DO UPDATE SET data = excluded.data
          `);
          stmt.run(item.id, Buffer.from(item.data));
        }
      });

      transaction();

      const elapsed = Date.now() - startTime;
      this.updateMetric('save', elapsed / items.length, items.reduce((sum, item) => sum + item.data.length, 0));

      logger.debug("Batch save completed", {
        table: this.getTableName(),
        count: items.length,
        totalTimeMs: elapsed,
      });
    } catch (error) {
      this.handleSqliteError(error, "saveBatch", { count: items.length });
    }
  }

  /**
   * Save multiple items with custom save logic (protected helper for subclasses)
   * @param items Array of items to save
   * @param saveFn Custom save function for each item
   */
  protected async saveBatchWithCustomLogic(
    items: Array<{ id: string; data: Uint8Array; metadata: any }>,
    saveFn: (db: Database.Database, item: { id: string; data: Uint8Array; metadata: any }) => void,
  ): Promise<void> {
    const db = this.getDb();
    const startTime = Date.now();

    try {
      // Use transaction for atomicity and performance
      const transaction = db.transaction(() => {
        for (const item of items) {
          saveFn(db, item);
        }
      });

      transaction();

      const elapsed = Date.now() - startTime;
      this.updateMetric('save', elapsed / items.length, items.reduce((sum, item) => sum + item.data.length, 0));

      logger.debug("Batch save completed", {
        table: this.getTableName(),
        count: items.length,
        totalTimeMs: elapsed,
      });
    } catch (error) {
      this.handleSqliteError(error, "saveBatch", { count: items.length });
    }
  }

  /**
   * Load multiple items efficiently
   * @param ids Array of IDs to load
   * @returns Array of loaded data (null if not found), maintaining order
   */
  async loadBatch(
    ids: string[],
  ): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    const db = this.getDb();
    const startTime = Date.now();

    try {
      if (ids.length === 0) {
        return [];
      }

      // Use IN clause for efficient batch loading
      const placeholders = ids.map(() => '?').join(',');
      const stmt = db.prepare(
        `SELECT id, data FROM ${this.getTableName()} WHERE id IN (${placeholders})`
      );
      const rows = stmt.all(...ids) as Array<{ id: string; data: Buffer }>;

      // Create a map for quick lookup
      const dataMap = new Map<string, Uint8Array>();
      for (const row of rows) {
        dataMap.set(row.id, new Uint8Array(row.data));
      }

      // Maintain order and handle missing items
      const results = ids.map(id => ({
        id,
        data: dataMap.get(id) || null,
      }));

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed / ids.length);

      logger.debug("Batch load completed", {
        table: this.getTableName(),
        requested: ids.length,
        found: rows.length,
        totalTimeMs: elapsed,
      });

      return results;
    } catch (error) {
      this.handleSqliteError(error, "loadBatch", { count: ids.length });
    }
  }

  /**
   * Load multiple items with custom extraction logic (protected helper for subclasses)
   * @param ids Array of IDs to load
   * @param loadFn Function to extract data from row
   * @returns Array of loaded data (null if not found), maintaining order
   */
  protected async loadBatchWithCustomLogic<T>(
    ids: string[],
    loadFn: (row: any) => Uint8Array | null,
  ): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    const db = this.getDb();
    const startTime = Date.now();

    try {
      if (ids.length === 0) {
        return [];
      }

      // Use IN clause for efficient batch loading
      const placeholders = ids.map(() => '?').join(',');
      const stmt = db.prepare(
        `SELECT id, data FROM ${this.getTableName()} WHERE id IN (${placeholders})`
      );
      const rows = stmt.all(...ids) as Array<{ id: string; data: Buffer }>;

      // Create a map for quick lookup
      const dataMap = new Map<string, Uint8Array>();
      for (const row of rows) {
        const extractedData = loadFn(row);
        if (extractedData) {
          dataMap.set(row.id, extractedData);
        }
      }

      // Maintain order and handle missing items
      const results = ids.map(id => ({
        id,
        data: dataMap.get(id) || null,
      }));

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed / ids.length);

      logger.debug("Batch load completed", {
        table: this.getTableName(),
        requested: ids.length,
        found: rows.length,
        totalTimeMs: elapsed,
      });

      return results;
    } catch (error) {
      this.handleSqliteError(error, "loadBatch", { count: ids.length });
    }
  }

  /**
   * Delete multiple items in a single transaction
   * More efficient than individual deletes for bulk operations
   * @param ids Array of IDs to delete
   */
  async deleteBatch(ids: string[]): Promise<void> {
    const db = this.getDb();
    const startTime = Date.now();

    try {
      if (ids.length === 0) {
        return;
      }

      // Use transaction for atomicity and performance
      const transaction = db.transaction(() => {
        const stmt = db.prepare(`DELETE FROM ${this.getTableName()} WHERE id = ?`);
        for (const id of ids) {
          stmt.run(id);
        }
      });

      transaction();

      const elapsed = Date.now() - startTime;
      this.updateMetric('delete', elapsed / ids.length);

      logger.debug("Batch delete completed", {
        table: this.getTableName(),
        count: ids.length,
        totalTimeMs: elapsed,
      });
    } catch (error) {
      this.handleSqliteError(error, "deleteBatch", { count: ids.length });
    }
  }
}
