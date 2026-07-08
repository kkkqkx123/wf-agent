/**
 * PostgreSQL-specific Key-Value Storage Base
 *
 * Provides PostgreSQL implementation of:
 * - executeQuery()
 * - executeTransaction()
 * - createSchema()
 */

import { Pool } from "pg";
import { KeyValueStorageBase } from "./key-value-storage-base.js";
import type { BasePostgresStorageConfig } from "../../postgres/base-postgres-storage.js";
import { createModuleLogger } from "../../logger.js";

const logger = createModuleLogger("postgres-key-value-storage");

export abstract class PostgresKeyValueStorageBase<TMetadata, TListOptions = void>
  extends KeyValueStorageBase<TMetadata, TListOptions>
{
  protected pool: Pool | null = null;
  protected postgresConfig: BasePostgresStorageConfig;

  constructor(config: BasePostgresStorageConfig) {
    super();
    this.postgresConfig = config;
  }

  /**
   * Get connection pool
   */
  protected getPool(): Pool {
    if (!this.pool) {
      throw new Error("Connection pool not initialized");
    }
    return this.pool;
  }

  /**
   * Initialize PostgreSQL connection pool
   */
  protected override async createSchema(): Promise<void> {
    this.pool = new Pool(this.postgresConfig);

    // Test connection
    const client = await this.pool.connect();
    try {
      await client.query("SELECT 1");
      logger.debug("PostgreSQL connection established");
    } finally {
      client.release();
    }

    // Create schema
    await this.createPostgresSchema();
  }

  /**
   * Create database-specific schema
   * Subclasses override to create their tables
   */
  protected abstract createPostgresSchema(): Promise<void>;

  /**
   * Execute query with PostgreSQL
   */
  protected override async executeQuery(sql: string, params?: any[]): Promise<any> {
    const pool = this.getPool();
    const client = await pool.connect();

    try {
      const result = await client.query(sql, params);
      return result.rows[0];
    } finally {
      client.release();
    }
  }

  /**
   * Execute transaction with PostgreSQL
   */
  protected override async executeTransaction(
    operations: Array<{ sql: string; params?: any[] }>,
  ): Promise<void> {
    const pool = this.getPool();
    const client = await pool.connect();

    try {
      await client.query("BEGIN");

      for (const op of operations) {
        await client.query(op.sql, op.params);
      }

      await client.query("COMMIT");
    } catch (error) {
      await client.query("ROLLBACK");
      throw error;
    } finally {
      client.release();
    }
  }

  /**
   * List with PostgreSQL
   */
  override async list(_options?: TListOptions): Promise<string[]> {
    const startTime = Date.now();
    this.ensureInitialized();

    try {
      const config = this.getConfig();
      const pool = this.getPool();
      const client = await pool.connect();

      try {
        const result = await client.query(
          `SELECT id FROM ${config.tableName} ORDER BY created_at DESC`,
        );

        const ids = result.rows.map((r) => r.id);

        const elapsed = Date.now() - startTime;
        this.updateMetric("list", elapsed);
        return ids;
      } finally {
        client.release();
      }
    } catch (error) {
      this.handleError(error, "list");
      throw error;
    }
  }

  /**
   * Close PostgreSQL connection pool
   */
  override async close(): Promise<void> {
    if (this.pool) {
      await this.pool.end();
      this.pool = null;
      logger.debug("PostgreSQL connection pool closed");
    }
  }
}
