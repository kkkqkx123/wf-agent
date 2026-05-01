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
}
