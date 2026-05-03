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
  /** Enable data integrity verification on load (default: false for performance) */
  verifyIntegrity?: boolean;
  /** Verify integrity every Nth load operation (default: 100, only used when verifyIntegrity is true) */
  integrityCheckFrequency?: number;
}

/**
 * Default pagination limits
 */
const DEFAULT_PAGE_LIMIT = 100;
const MAX_PAGE_LIMIT = 1000;

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
  protected loadCounter: number = 0; // Counter for integrity check frequency

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

      // Disable connection pool when fileMustExist is true to ensure proper error handling
      const shouldUsePool = usePool && !this.config.readonly && !(this.config.fileMustExist ?? false);

      if (shouldUsePool) {
        // Use connection pool
        this.connectionPool = this.config.connectionPool ?? getGlobalConnectionPool();
        this.db = this.connectionPool.getConnection(this.config.dbPath);
        this.usingPool = true;
        logger.debug("Using pooled SQLite connection", { dbPath: this.config.dbPath });
      } else {
        // Create dedicated connection (for readonly, fileMustExist, or when pool is disabled)
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
        this.db.pragma("foreign_keys = ON");

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
   * Compute hash for BLOB data using Web Crypto API
   * Uses sampling for large objects to improve performance
   * Can be overridden by subclasses for different hash algorithms
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
   * @param data The data to verify
   * @param expectedHash The expected hash from metadata
   * @param id The ID of the data (for logging)
   * @throws StorageError if integrity check fails
   */
  protected async verifyIntegrity(
    data: Uint8Array,
    expectedHash: string,
    id: string,
  ): Promise<void> {
    const actualHash = await this.computeHash(data);
    if (actualHash !== expectedHash) {
      logger.error("Data integrity check failed", { 
        id, 
        expected: expectedHash, 
        actual: actualHash 
      });
      throw new StorageError(
        "Data integrity verification failed",
        "load",
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
      // Zero-copy conversion: use shared ArrayBuffer to avoid memory duplication
      const buffer = row.data;
      return new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
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
   * Optimize database by running VACUUM and ANALYZE
   * Should be called periodically to maintain performance
   */
  async optimize(): Promise<void> {
    const db = this.getDb();

    try {
      logger.info("Starting database optimization", { table: this.getTableName() });
      
      // Reclaim unused space
      db.exec('VACUUM');
      logger.debug("VACUUM completed", { table: this.getTableName() });
      
      // Update query planner statistics
      db.exec('ANALYZE');
      logger.debug("ANALYZE completed", { table: this.getTableName() });
      
      logger.info("Database optimization completed", { table: this.getTableName() });
    } catch (error) {
      this.handleSqliteError(error, "optimize", {});
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
      // Prepare statement once outside loop for better performance
      const stmt = db.prepare(`
        INSERT INTO ${this.getTableName()} (id, data) VALUES (?, ?)
        ON CONFLICT(id) DO UPDATE SET data = excluded.data
      `);

      // Use transaction for atomicity and performance
      const transaction = db.transaction(() => {
        for (const item of items) {
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
      logger.error("Batch save failed", { 
        table: this.getTableName(), 
        count: items.length,
        error: (error as Error).message 
      });
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
        // Zero-copy conversion: use shared ArrayBuffer to avoid memory duplication
        const buffer = row.data;
        dataMap.set(row.id, new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength));
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

      // Prepare statement once outside loop for better performance
      const stmt = db.prepare(`DELETE FROM ${this.getTableName()} WHERE id = ?`);

      // Use transaction for atomicity and performance
      const transaction = db.transaction(() => {
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
      logger.error("Batch delete failed", { 
        table: this.getTableName(), 
        count: ids.length,
        error: (error as Error).message 
      });
      this.handleSqliteError(error, "deleteBatch", { count: ids.length });
    }
  }
}
