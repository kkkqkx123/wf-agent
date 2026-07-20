/**
 * SQLite-specific Key-Value Storage Base
 *
 * Provides SQLite implementation of:
 * - executeQuery()
 * - executeTransaction()
 * - createSchema()
 *
 * Subclasses only need to define:
 * 1. getConfig() - Table name and metadata converters
 * 2. getSqliteConfig() - SQLite-specific settings (dbPath, pragma, etc.)
 */

import Database from "better-sqlite3";
import { KeyValueStorageBase } from "./key-value-storage-base.js";
import type { BaseSqliteStorageConfig } from "../../sqlite/base-sqlite-storage.js";
import { createModuleLogger } from "../../logger.js";

const logger = createModuleLogger("sqlite-key-value-storage");

export abstract class SqliteKeyValueStorageBase<TMetadata, TListOptions = void>
  extends KeyValueStorageBase<TMetadata, TListOptions>
{
  protected db: Database.Database | null = null;
  protected sqliteConfig: BaseSqliteStorageConfig;
  /** Whether the db connection was injected externally (shared connection) */
  private externalConnection: boolean = false;

  constructor(config: BaseSqliteStorageConfig) {
    super();
    this.sqliteConfig = config;
  }

  /**
   * Inject an external database connection (shared connection).
   * When set, createSchema() will skip creating a new connection and
   * close() will NOT close the connection (the owner is responsible).
   */
  setExternalDb(db: Database.Database): void {
    this.db = db;
    this.externalConnection = true;
  }

  /**
   * Get SQLite database instance
   */
  protected getDb(): Database.Database {
    if (!this.db) {
      throw new Error("Database not initialized");
    }
    return this.db;
  }

  /**
   * Initialize SQLite database
   */
  protected override async createSchema(): Promise<void> {
    // Use externally injected shared connection if available
    if (!this.externalConnection) {
      // Open database connection
      const options: Database.Options = {
        readonly: this.sqliteConfig.readonly ?? false,
        fileMustExist: this.sqliteConfig.fileMustExist ?? false,
        timeout: this.sqliteConfig.timeout ?? 5000,
      };

      this.db = new Database(this.sqliteConfig.dbPath, options);

      // Apply pragmas for performance
      this.db.pragma("journal_mode = WAL");
      this.db.pragma("synchronous = NORMAL");
      this.db.pragma("cache_size = -64000"); // 64MB
      this.db.pragma("foreign_keys = ON");
    } else {
      logger.debug("Using shared external SQLite connection for key-value storage", {
        dbPath: this.sqliteConfig.dbPath,
      });
    }

    // Create schema (IF NOT EXISTS ensures idempotency even with shared connection)
    await this.createSqliteSchema();
  }

  /**
   * Create database-specific schema
   * Subclasses override to create their tables
   */
  protected abstract createSqliteSchema(): void;

  /**
   * Execute query with SQLite
   * Uses stmt.get() for SELECT queries, stmt.run() for DML (INSERT/UPDATE/DELETE)
   */
  protected override async executeQuery(sql: string, params?: any[]): Promise<any> {
    const db = this.getDb();
    const stmt = db.prepare(sql);
    const trimmedSql = sql.trim().toUpperCase();
    if (trimmedSql.startsWith("SELECT")) {
      return stmt.get(...(params || []));
    }
    return stmt.run(...(params || []));
  }

  /**
   * Execute transaction with SQLite
   */
  protected override async executeTransaction(
    operations: Array<{ sql: string; params?: any[] }>,
  ): Promise<void> {
    const db = this.getDb();
    const transaction = db.transaction(() => {
      for (const op of operations) {
        const stmt = db.prepare(op.sql);
        stmt.run(...(op.params || []));
      }
    });

    transaction();
  }

  /**
   * List with SQLite
   */
  override async list(_options?: TListOptions): Promise<string[]> {
    const startTime = Date.now();
    this.ensureInitialized();

    try {
      const config = this.getConfig();
      const db = this.getDb();
      const stmt = db.prepare(`SELECT id FROM ${config.tableName} ORDER BY created_at DESC`);
      const rows = stmt.all() as Array<{ id: string }>;
      const ids = rows.map((r) => r.id);

      const elapsed = Date.now() - startTime;
      this.updateMetric("list", elapsed);
      return ids;
    } catch (error) {
      this.handleError(error, "list");
      throw error;
    }
  }

  /**
   * Close SQLite connection
   */
  override async close(): Promise<void> {
    if (this.db) {
      // Only close the connection if we own it (not an externally injected shared connection)
      if (!this.externalConnection) {
        this.db.close();
        logger.debug("SQLite connection closed");
      } else {
        logger.debug("Skipping close for externally managed shared connection");
      }
      this.db = null;
      this.initialized = false;
    }
  }
}
