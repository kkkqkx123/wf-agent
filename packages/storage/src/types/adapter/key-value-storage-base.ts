/**
 * Generic Key-Value Storage Adapter Base
 *
 * Template method pattern to eliminate code duplication across
 * SQLite, PostgreSQL, JSON and other simple key-value stores.
 *
 * Subclasses define only what's unique:
 * - Table schema
 * - Metadata column definitions
 * - Data type conversions (DB ↔ TypeScript)
 * - Query execution (delegated to each DB's API)
 *
 * Base class provides all standard CRUD operations.
 */

import type { BaseStorageAdapter } from "./base-storage-adapter.js";
import type { StorageMetrics } from "../metrics.js";
import { DEFAULT_STORAGE_METRICS } from "../metrics.js";
import { StorageError } from "../storage-errors.js";
import { createModuleLogger } from "../../logger.js";

const logger = createModuleLogger("key-value-storage-base");

/**
 * Configuration for a key-value storage backend
 * Defines what makes each storage type unique
 */
export interface KeyValueStorageConfig<TMetadata> {
  /** Primary metadata table name */
  tableName: string;

  /** Optional BLOB table name (null = store content inline) */
  blobTableName: string | null;

  /** Convert database row to TypeScript metadata object */
  rowToMetadata: (row: any) => TMetadata;

  /** Convert TypeScript metadata to database values */
  metadataToValues: (metadata: TMetadata) => Record<string, any>;
}

/**
 * Query result interface for different databases
 */
export interface QueryResult<T = any> {
  rows?: T[];
  rowCount?: number;
  [key: string]: any;
}

/**
 * Base class for simple key-value storage implementations
 * Implements all standard CRUD operations using template methods.
 *
 * Subclasses must implement:
 * 1. getConfig() - Define table and metadata schema
 * 2. createSchema() - Create database tables
 * 3. executeQuery() - Execute SQL with their DB library
 * 4. executeBatch() - Execute multiple queries (optional)
 */
export abstract class KeyValueStorageBase<TMetadata, TListOptions = void>
  implements BaseStorageAdapter<TMetadata, TListOptions>
{
  protected initialized: boolean = false;
  protected metrics: StorageMetrics = { ...DEFAULT_STORAGE_METRICS };

  /**
   * Get storage configuration
   * Subclasses override to define table names and converters
   */
  protected abstract getConfig(): KeyValueStorageConfig<TMetadata>;

  /**
   * Create database schema
   * Subclasses override to create their tables
   */
  protected abstract createSchema(): Promise<void>;

  /**
   * Execute a single query
   * Subclasses delegate to their database library (better-sqlite3, pg, etc.)
   *
   * @param sql SQL query string
   * @param params Query parameters
   * @returns Query result (structure varies by DB)
   */
  protected abstract executeQuery(sql: string, params?: any[]): Promise<any>;

  /**
   * Execute multiple queries as a transaction
   * Subclasses wrap queries in BEGIN/COMMIT/ROLLBACK
   */
  protected abstract executeTransaction(
    operations: Array<{ sql: string; params?: any[] }>,
  ): Promise<void>;

  /**
   * Initialize storage backend
   */
  async initialize(): Promise<void> {
    logger.debug("Initializing key-value storage");
    try {
      await this.createSchema();
      this.initialized = true;
      logger.info("Key-value storage initialized");
    } catch (error) {
      logger.error("Failed to initialize storage", { error: (error as Error).message });
      throw new StorageError("Failed to initialize storage", "initialize");
    }
  }

  /**
   * Save data to storage
   */
  async save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void> {
    const startTime = Date.now();
    this.ensureInitialized();

    try {
      const config = this.getConfig();
      const metaValues = config.metadataToValues(metadata);

      const operations: Array<{ sql: string; params?: any[] }> = [];

      // Insert/update metadata
      const columns = Object.keys(metaValues);
      const placeholders = columns.map(() => "?").join(", ");
      const updates = columns.map((c) => `${c} = excluded.${c}`).join(", ");

      operations.push({
        sql: `
          INSERT INTO ${config.tableName} (id, ${columns.join(", ")})
          VALUES (?, ${placeholders})
          ON CONFLICT(id) DO UPDATE SET ${updates}
        `,
        params: [id, ...Object.values(metaValues)],
      });

      // Insert/update BLOB if table is separated
      if (config.blobTableName) {
        const blobIdCol = config.tableName.slice(0, -1) + "_id";
        operations.push({
          sql: `
            INSERT INTO ${config.blobTableName} (${blobIdCol}, content)
            VALUES (?, ?)
            ON CONFLICT(${blobIdCol}) DO UPDATE SET content = excluded.content
          `,
          params: [id, data],
        });
      }

      await this.executeTransaction(operations);

      const elapsed = Date.now() - startTime;
      this.updateMetric("save", elapsed, data.length);
      logger.debug("Data saved", { id, size: data.length });
    } catch (error) {
      this.handleError(error, "save", { id });
      throw error;
    }
  }

  /**
   * Load data from storage
   */
  async load(id: string): Promise<Uint8Array | null> {
    const startTime = Date.now();
    this.ensureInitialized();

    try {
      const config = this.getConfig();

      let sql: string;
      if (config.blobTableName) {
        const blobIdCol = config.tableName.slice(0, -1) + "_id";
        sql = `SELECT content FROM ${config.blobTableName} WHERE ${blobIdCol} = ?`;
      } else {
        sql = `SELECT content FROM ${config.tableName} WHERE id = ?`;
      }

      const row = await this.executeQuery(sql, [id]);
      if (!row || !row.content) {
        return null;
      }

      // Handle both Buffer (SQLite/PostgreSQL) and Uint8Array
      const content = row.content instanceof Uint8Array ? row.content : Buffer.from(row.content);
      const data = new Uint8Array(content);

      const elapsed = Date.now() - startTime;
      this.updateMetric("load", elapsed, data.length);
      return data;
    } catch (error) {
      this.handleError(error, "load", { id });
      throw error;
    }
  }

  /**
   * Delete data from storage
   */
  async delete(id: string): Promise<void> {
    const startTime = Date.now();
    this.ensureInitialized();

    try {
      const config = this.getConfig();
      await this.executeQuery(`DELETE FROM ${config.tableName} WHERE id = ?`, [id]);

      const elapsed = Date.now() - startTime;
      this.updateMetric("delete", elapsed);
      logger.debug("Data deleted", { id });
    } catch (error) {
      this.handleError(error, "delete", { id });
      throw error;
    }
  }

  /**
   * Check if data exists
   */
  async exists(id: string): Promise<boolean> {
    this.ensureInitialized();

    try {
      const config = this.getConfig();
      const row = await this.executeQuery(`SELECT 1 FROM ${config.tableName} WHERE id = ?`, [id]);
      return !!row;
    } catch (error) {
      this.handleError(error, "exists", { id });
      throw error;
    }
  }

  /**
   * List all IDs
   */
  async list(_options?: TListOptions): Promise<string[]> {
    const startTime = Date.now();
    this.ensureInitialized();

    try {
      const config = this.getConfig();
      const rows = await this.executeQuery(
        `SELECT id FROM ${config.tableName} ORDER BY created_at DESC`,
      );

      const ids = Array.isArray(rows) ? rows.map((r: any) => r.id) : [];

      const elapsed = Date.now() - startTime;
      this.updateMetric("list", elapsed);
      return ids;
    } catch (error) {
      this.handleError(error, "list");
      throw error;
    }
  }

  /**
   * Get metadata for an item
   */
  async getMetadata(id: string): Promise<TMetadata | null> {
    this.ensureInitialized();

    try {
      const config = this.getConfig();
      const row = await this.executeQuery(
        `SELECT * FROM ${config.tableName} WHERE id = ?`,
        [id],
      );

      if (!row) {
        return null;
      }

      return config.rowToMetadata(row);
    } catch (error) {
      this.handleError(error, "getMetadata", { id });
      throw error;
    }
  }

  /**
   * Clear all data
   */
  async clear(): Promise<void> {
    this.ensureInitialized();

    try {
      const config = this.getConfig();
      await this.executeQuery(`DELETE FROM ${config.tableName}`);
      logger.info("Storage cleared");
    } catch (error) {
      this.handleError(error, "clear");
      throw error;
    }
  }

  /**
   * Close storage connection (implement if needed)
   */
  async close(): Promise<void> {
    // Optional - subclasses override if they need to close connections
  }

  /**
   * Get storage metrics
   */
  async getMetrics(): Promise<StorageMetrics> {
    return { ...this.metrics };
  }

  /**
   * Reset storage metrics
   */
  resetMetrics(): void {
    this.metrics = { ...DEFAULT_STORAGE_METRICS };
  }

  /**
   * Batch save operations (can override for optimization)
   */
  async saveBatch(items: Array<{ id: string; data: Uint8Array; metadata: TMetadata }>): Promise<void> {
    for (const item of items) {
      await this.save(item.id, item.data, item.metadata);
    }
  }

  /**
   * Batch load operations (can override for optimization)
   */
  async loadBatch(ids: string[]): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    return Promise.all(ids.map(async (id) => ({ id, data: await this.load(id) })));
  }

  /**
   * Batch delete operations (can override for optimization)
   */
  async deleteBatch(ids: string[]): Promise<void> {
    for (const id of ids) {
      await this.delete(id);
    }
  }

  // ────────────────────────────────────────────────────────────────
  // Protected helpers
  // ────────────────────────────────────────────────────────────────

  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw new StorageError("Storage not initialized. Call initialize() first.", "initialize");
    }
  }

  protected updateMetric(operation: string, timeMs: number, dataSize?: number): void {
    const countKey = `${operation}Count` as keyof StorageMetrics;
    const timeKey = `avg${operation.charAt(0).toUpperCase()}${operation.slice(1)}Time` as keyof StorageMetrics;

    this.metrics[countKey] = (this.metrics[countKey] as number) + 1;

    const currentAvg = this.metrics[timeKey] as number;
    const count = this.metrics[countKey] as number;
    this.metrics[timeKey] = currentAvg + (timeMs - currentAvg) / count;

    if (dataSize !== undefined) {
      this.metrics.totalBlobSize += dataSize;
    }
  }

  protected handleError(error: any, operation: string, context?: Record<string, any>): void {
    logger.error(`Storage ${operation} failed`, {
      error: (error as Error).message,
      ...context,
    });
  }
}
