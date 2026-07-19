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

  constructor(config: BaseSqliteStorageConfig) {
    super();
    this.sqliteConfig = config;
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

    // Create schema
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
      this.db.close();
      this.db = null;
      this.initialized = false;
      logger.debug("SQLite connection closed");
    }
  }
}
