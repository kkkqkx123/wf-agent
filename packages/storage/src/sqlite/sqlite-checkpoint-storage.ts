/**
 * SQLite Checkpoint Storage Implementation with Metadata-BLOB Separation
 * Checkpoint persistent storage based on better-sqlite3
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type { CheckpointStorageMetadata, CheckpointStorageListOptions, CheckpointEntityType } from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "../types/adapter/index.js";
import type { CheckpointOptions } from "../types/checkpoint-options.js";
import { BaseSqliteStorage, BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
import { selectCompressionStrategy } from "@wf-agent/common-utils";
import { compressBlob, decompressBlob, compressBlobSync, decompressBlobSync } from "@wf-agent/common-utils";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("sqlite-checkpoint-storage");

/**
 * SQLite Checkpoint Storage
 * Implementing the CheckpointStorageAdapter interface with metadata-BLOB separation
 */
export class SqliteCheckpointStorage
  extends BaseSqliteStorage<CheckpointStorageMetadata, CheckpointStorageListOptions>
  implements CheckpointStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
    super(config);
  }

  /**
   * Get metadata table name
   */
  protected getTableName(): string {
    return "checkpoint_metadata";
  }

  /**
   * Get BLOB table name
   */
  protected getBlobTableName(): string | null {
    return "checkpoint_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   * Optimized for entity-based queries with composite indexes
   */
  protected createTableSchema(): void {
    const db = this.getDb();

    // Layer 1: Metadata table (frequent queries, no BLOB)
    db.exec(`
      CREATE TABLE IF NOT EXISTS checkpoint_metadata (
        id TEXT PRIMARY KEY,
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        checkpoint_type TEXT,
        base_checkpoint_id TEXT,
        previous_checkpoint_id TEXT,
        message_count INTEGER,
        variable_count INTEGER,
        blob_size INTEGER,
        blob_hash TEXT,
        tags TEXT CHECK(length(tags) <= 4096),
        custom_fields TEXT CHECK(length(custom_fields) <= 8192),
        created_at INTEGER NOT NULL DEFAULT ((strftime('%s', 'now') * 1000)),
        updated_at INTEGER NOT NULL DEFAULT ((strftime('%s', 'now') * 1000))
      )
    `);

    // Layer 2: BLOB storage table (infrequent direct access)
    db.exec(`
      CREATE TABLE IF NOT EXISTS checkpoint_blob (
        checkpoint_id TEXT PRIMARY KEY,
        blob_data BLOB NOT NULL,
        compressed INTEGER DEFAULT 0 CHECK(compressed IN (0, 1)),
        compression_algorithm TEXT,
        CHECK(
          (compressed = 0 AND compression_algorithm IS NULL) OR
          (compressed = 1 AND compression_algorithm IS NOT NULL)
        ),
        FOREIGN KEY (checkpoint_id) REFERENCES checkpoint_metadata(id) ON DELETE CASCADE
      )
    `);

    // ========================================================================
    // Optimized Indexes for Entity-Based Queries
    // ========================================================================
    
    // Primary composite index: entity lookup with timestamp ordering
    // Supports: listByEntityWithMetadata, getLatestByEntity, cleanup by entity
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_entity_timestamp 
       ON checkpoint_metadata(entity_type, entity_id, timestamp DESC)`
    );

    // Timestamp-only index: time-based cleanup and range queries
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_timestamp 
       ON checkpoint_metadata(timestamp)`
    );

    // Checkpoint type index: filter by FULL/DELTA
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_type 
       ON checkpoint_metadata(checkpoint_type)`
    );

    // Hash index: deduplication detection
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_blob_hash 
       ON checkpoint_metadata(blob_hash)`
    );

    // Entity metadata table: stores per-entity key-value metadata (e.g., cleanup watermark)
    db.exec(`
      CREATE TABLE IF NOT EXISTS entity_metadata (
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        metadata_key TEXT NOT NULL,
        metadata_value TEXT,
        updated_at INTEGER NOT NULL DEFAULT ((strftime('%s', 'now') * 1000)),
        PRIMARY KEY (entity_type, entity_id, metadata_key)
      )
    `);

    logger.debug('Created optimized indexes for entity-based queries', {
      table: this.getTableName()
    });
  }

  /**
   * Extract metrics from checkpoint data for metadata storage
   *
   * Uses safe field access with fallback to avoid parsing errors on different checkpoint structures.
   * Supports both workflow checkpoints (with executionState) and agent checkpoints (with snapshot).
   */
  private async extractMetrics(data: Uint8Array): Promise<{
    messageCount: number;
    variableCount: number;
    blobHash: string;
  }> {
    try {
      const decoder = new TextDecoder();
      const jsonStr = decoder.decode(data);
      const checkpoint = JSON.parse(jsonStr);

      // Support multiple checkpoint structures
      // Workflow: checkpoint.executionState.conversationState.messages
      // Agent: checkpoint.snapshot.conversationState.messages (or similar)
      const executionState = checkpoint.executionState ?? checkpoint.snapshot;
      const conversationState = executionState?.conversationState;

      let messageCount = 0;
      let variableCount = 0;

      if (conversationState?.messages) {
        messageCount = Array.isArray(conversationState.messages) ? conversationState.messages.length : 0;
      }

       if (executionState?.variableState) {
         const variables = executionState.variableState.variables;
         if (variables && typeof variables === 'object') {
           // variables is a Record<string, unknown>, count its keys
           variableCount = Object.keys(variables).length;
         }
       } else if (executionState?.variables) {
         // Fallback for older checkpoint formats
         const variables = executionState.variables;
         if (variables && typeof variables === 'object') {
           variableCount = Object.keys(variables).length;
         }
       }

      const blobHash = await this.computeHash(data);

      return { messageCount, variableCount, blobHash };
    } catch (error) {
      logger.warn("Failed to extract checkpoint metrics", {
        error: (error as Error).message,
      });
      return {
        messageCount: 0,
        variableCount: 0,
        blobHash: await this.computeHash(data),
      };
    }
  }

  /**
   * Save checkpoint with metadata-BLOB separation and compression
   * @param id Checkpoint ID
   * @param data Checkpoint data
   * @param metadata Checkpoint metadata
   * @param options Checkpoint options (sync mode, timeout, etc.)
   */
  async save(
    id: string,
    data: Uint8Array,
    metadata: CheckpointStorageMetadata,
    options?: CheckpointOptions,
  ): Promise<void> {
    const startTime = Date.now();
    const db = this.getDb();
    const now = Date.now();

    try {
      // Extract metrics from data
      const metrics = await this.extractMetrics(data);

      // Get compression config based on data characteristics
      const config = selectCompressionStrategy(data);

      // Compress BLOB data
      const { compressed, algorithm } = await compressBlob(data, config);

      // Use transaction to ensure atomicity
      const insertMetadata = db.prepare(`
        INSERT INTO checkpoint_metadata (
          id, entity_type, entity_id, timestamp, checkpoint_type,
          base_checkpoint_id, previous_checkpoint_id, message_count, variable_count,
          blob_size, blob_hash, tags, custom_fields, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          entity_type = excluded.entity_type,
          entity_id = excluded.entity_id,
          timestamp = excluded.timestamp,
          checkpoint_type = excluded.checkpoint_type,
          base_checkpoint_id = excluded.base_checkpoint_id,
          previous_checkpoint_id = excluded.previous_checkpoint_id,
          message_count = excluded.message_count,
          variable_count = excluded.variable_count,
          blob_size = excluded.blob_size,
          blob_hash = excluded.blob_hash,
          tags = excluded.tags,
          custom_fields = excluded.custom_fields,
          updated_at = excluded.updated_at
      `);

      const insertBlob = db.prepare(`
        INSERT INTO checkpoint_blob (checkpoint_id, blob_data, compressed, compression_algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(checkpoint_id) DO UPDATE SET
          blob_data = excluded.blob_data,
          compressed = excluded.compressed,
          compression_algorithm = excluded.compression_algorithm
      `);

      db.transaction(() => {
        insertMetadata.run(
          id,
          metadata.entityType,
          metadata.entityId,
          metadata.timestamp,
          metadata.checkpointType ?? null,
          metadata.baseCheckpointId ?? null,
          metadata.previousCheckpointId ?? null,
          metrics.messageCount,
          metrics.variableCount,
          compressed.length,
          metrics.blobHash,
          metadata.tags ? JSON.stringify(metadata.tags) : null,
          metadata.customFields ? JSON.stringify(metadata.customFields) : null,
          now,
          now,
        );

        insertBlob.run(id, compressed, algorithm ? 1 : 0, algorithm || null);
      })();

      // If sync mode is enabled, force WAL checkpoint to ensure data is flushed to disk
      if (options?.sync) {
        try {
          db.pragma('wal_checkpoint(TRUNCATE)');
          logger.debug('Synchronous checkpoint saved with WAL flush', { id, size: data.length });
        } catch (error) {
          logger.error('Failed to flush WAL during synchronous checkpoint', { 
            id, 
            error: (error as Error).message 
          });
          throw error;
        }
      }

      // Track metrics
      const elapsed = Date.now() - startTime;
      this.updateMetric('save', elapsed, compressed.length);
    } catch (error) {
      this.handleSqliteError(error, "save", { id });
      throw error; // Re-throw to ensure caller knows about the failure
    }
  }

  /**
   * Load checkpoint data with automatic decompression
   */
  override async load(id: string): Promise<Uint8Array | null> {
    const startTime = Date.now();
    const db = this.getDb();

    try {
      // Get both blob data and metadata for integrity verification
      const stmt = db.prepare(`
        SELECT cb.blob_data, cb.compressed, cb.compression_algorithm, cm.blob_hash
        FROM checkpoint_blob cb
        INNER JOIN checkpoint_metadata cm ON cb.checkpoint_id = cm.id
        WHERE cb.checkpoint_id = ?
      `);
      const row = stmt.get(id) as
        | {
            blob_data: Buffer;
            compressed: number;
            compression_algorithm: string | null;
            blob_hash: string;
          }
        | undefined;

      if (!row) {
        return null;
      }

      // Zero-copy conversion: use shared ArrayBuffer to avoid memory duplication
      const buffer = row.blob_data;
      const data = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);

      // Decompress if needed
      let finalData: Uint8Array;
      if (row.compressed && row.compression_algorithm) {
        finalData = await decompressBlob(data, row.compression_algorithm);
      } else {
        finalData = data;
      }

      // Optional integrity verification
      if (this.shouldVerifyIntegrity()) {
        await this.verifyIntegrity(finalData, row.blob_hash, id);
      }

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed, finalData.length);
      return finalData;
    } catch (error) {
      this.handleSqliteError(error, "load", { id });
      throw error;
    }
  }

  /**
   * Delete checkpoint (cascade delete will handle blob)
   */
  override async delete(id: string): Promise<void> {
    const startTime = Date.now();
    const db = this.getDb();

    try {
      // Due to ON DELETE CASCADE, deleting from metadata will also delete from blob table
      const stmt = db.prepare(`DELETE FROM checkpoint_metadata WHERE id = ?`);
      stmt.run(id);
      
      const elapsed = Date.now() - startTime;
      this.updateMetric('delete', elapsed);
    } catch (error) {
      this.handleSqliteError(error, "delete", { id });
      throw error;
    }
  }

  /**
   * Check if checkpoint exists (only check metadata table)
   */
  override async exists(id: string): Promise<boolean> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT 1 FROM checkpoint_metadata WHERE id = ?`);
      const row = stmt.get(id);
      return row !== undefined;
    } catch (error) {
      this.handleSqliteError(error, "exists", { id });
      throw error;
    }
  }

  /**
   * List checkpoint IDs (optimized - only scans metadata table)
   */
  async list(options?: CheckpointStorageListOptions): Promise<string[]> {
    const startTime = Date.now();
    const db = this.getDb();

    try {
      let sql = `SELECT id FROM checkpoint_metadata`;
      const params: unknown[] = [];
      const conditions: string[] = [];

      // Construct filter criteria
      if (options?.entityType) {
        conditions.push("entity_type = ?");
        params.push(options.entityType);
      }

      if (options?.entityId) {
        conditions.push("entity_id = ?");
        params.push(options.entityId);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`EXISTS (SELECT 1 FROM json_each(tags) AS j WHERE j.value IN (${options.tags.map(() => "?").join(", ")}))`);
        params.push(...options.tags);
      }

      if (options?.type) {
        conditions.push("checkpoint_type = ?");
        params.push(options.type);
      }

      if (options?.timestampFrom) {
        conditions.push("timestamp >= ?");
        params.push(options.timestampFrom);
      }

      if (options?.timestampTo) {
        conditions.push("timestamp <= ?");
        params.push(options.timestampTo);
      }

if (conditions.length > 0) {
        sql += " WHERE " + conditions.join(" AND ");
      }

      // Add sorting options if specified
      if (options?.sortBy) {
        const order = options.sortOrder === 'asc' ? 'ASC' : 'DESC';
        switch (options.sortBy) {
          case 'timestamp':
            sql += ` ORDER BY timestamp ${order}`;
            break;
          case 'size':
            sql += ` ORDER BY blob_size ${order}`;
            break;
          case 'id':
            sql += ` ORDER BY id ${order}`;
            break;
        }
      } else {
        // Default sort by timestamp descending
        sql += " ORDER BY timestamp DESC";
      }

      // Pagination with validation
      const { limit: validatedLimit, offset: validatedOffset } = this.validatePagination(
        options?.limit,
        options?.offset
      );

      if (options?.limit !== undefined) {
        sql += " LIMIT ?";
        params.push(validatedLimit);
      }

      if (options?.offset !== undefined) {
        sql += " OFFSET ?";
        params.push(validatedOffset);
      }

      const stmt = db.prepare(sql);
      const rows = stmt.all(...params) as Array<{ id: string }>;

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      return rows.map(row => row.id);
    } catch (error) {
      this.handleSqliteError(error, "list", { options });
      throw error;
    }
  }

  /**
   * Get metadata (optimized - only reads metadata table)
   */
  async getMetadata(id: string): Promise<CheckpointStorageMetadata | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          entity_type as "entityType",
          entity_id as "entityId",
          timestamp,
          tags,
          custom_fields as "customFields"
        FROM checkpoint_metadata WHERE id = ?
      `);
      const row = stmt.get(id) as
        | {
            entityType: string;
            entityId: string;
            timestamp: number;
            tags: string | null;
            customFields: string | null;
          }
        | undefined;

      if (!row) {
        return null;
      }

      return {
        entityType: row.entityType as CheckpointEntityType,
        entityId: row.entityId,
        timestamp: row.timestamp,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.customFields ? JSON.parse(row.customFields) : undefined,
      };
    } catch (error) {
      this.handleSqliteError(error, "getMetadata", { id });
      throw error;
    }
  }

  /**
   * List checkpoints with metadata only (without loading BLOB data)
   * More efficient for cleanup operations
   */
  async listWithMetadata(options?: CheckpointStorageListOptions): Promise<Array<{
    id: string;
    metadata: CheckpointStorageMetadata;
  }>> {
    const startTime = Date.now();
    const db = this.getDb();

    try {
      let sql = `
        SELECT 
          id,
          entity_type as "entityType",
          entity_id as "entityId",
          timestamp,
          tags,
          custom_fields as "customFields",
          blob_size as "blobSize"
        FROM checkpoint_metadata`;
      const params: unknown[] = [];
      const conditions: string[] = [];

      // Construct filter criteria
      if (options?.entityType) {
        conditions.push("entity_type = ?");
        params.push(options.entityType);
      }

      if (options?.entityId) {
        conditions.push("entity_id = ?");
        params.push(options.entityId);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`EXISTS (SELECT 1 FROM json_each(tags) AS j WHERE j.value IN (${options.tags.map(() => "?").join(", ")}))`);
        params.push(...options.tags);
      }

if (conditions.length > 0) {
        sql += " WHERE " + conditions.join(" AND ");
      }

      // Add sorting options if specified
      if (options?.sortBy) {
        const order = options.sortOrder === 'asc' ? 'ASC' : 'DESC';
        switch (options.sortBy) {
          case 'timestamp':
            sql += ` ORDER BY timestamp ${order}`;
            break;
          case 'size':
            sql += ` ORDER BY blob_size ${order}`;
            break;
          case 'id':
            sql += ` ORDER BY id ${order}`;
            break;
        }
      } else {
        // Default sort by timestamp descending
        sql += " ORDER BY timestamp DESC";
      }

      // Pagination with validation
      const { limit: validatedLimit, offset: validatedOffset } = this.validatePagination(
        options?.limit,
        options?.offset
      );

      if (options?.limit !== undefined) {
        sql += " LIMIT ?";
        params.push(validatedLimit);
      }

      if (options?.offset !== undefined) {
        sql += " OFFSET ?";
        params.push(validatedOffset);
      }

      const stmt = db.prepare(sql);
      const rows = stmt.all(...params) as Array<{
        id: string;
        entityType: string;
        entityId: string;
        timestamp: number;
        tags: string | null;
        customFields: string | null;
        blobSize: number | null;
      }>;

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      return rows.map(row => ({
        id: row.id,
        metadata: {
          entityType: row.entityType as CheckpointEntityType,
          entityId: row.entityId,
          timestamp: row.timestamp,
          tags: row.tags ? JSON.parse(row.tags) : undefined,
          customFields: row.customFields ? JSON.parse(row.customFields) : undefined,
          blobSize: row.blobSize ?? 0,
        },
      }));
    } catch (error) {
      this.handleSqliteError(error, "listWithMetadata", { options });
      throw error;
    }
  }

  /**
   * Clear all checkpoints
   */
  override async clear(): Promise<void> {
    const db = this.getDb();

    try {
      // Due to ON DELETE CASCADE, clearing metadata will also clear blob table
      db.exec(`DELETE FROM checkpoint_metadata`);
      db.exec(`DELETE FROM entity_metadata`);
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
      throw error;
    }
  }

  /**
   * Get storage statistics
   */
  async getStats(): Promise<{
    totalCount: number;
    totalBlobSize: number;
    avgBlobSize: number;
    maxBlobSize: number;
  }> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          COUNT(*) as total_count,
          SUM(blob_size) as total_blob_size,
          AVG(blob_size) as avg_blob_size,
          MAX(blob_size) as max_blob_size
        FROM checkpoint_metadata
      `);
      const row = stmt.get() as {
        total_count: number;
        total_blob_size: number;
        avg_blob_size: number;
        max_blob_size: number;
      };

      return {
        totalCount: row.total_count || 0,
        totalBlobSize: row.total_blob_size || 0,
        avgBlobSize: Math.round(row.avg_blob_size || 0),
        maxBlobSize: row.max_blob_size || 0,
      };
    } catch (error) {
      this.handleSqliteError(error, "getStats", {});
      throw error;
    }
  }

  /**
   * Get entity-level metadata (e.g., cleanup watermark)
   * Uses the entity_metadata table for key-value storage per entity.
   */
  async getEntityMetadata(entityType: string, entityId: string): Promise<Record<string, unknown> | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT metadata_key, metadata_value FROM entity_metadata
        WHERE entity_type = ? AND entity_id = ?
      `);
      const rows = stmt.all(entityType, entityId) as Array<{ metadata_key: string; metadata_value: string | null }>;

      if (rows.length === 0) return null;

      const result: Record<string, unknown> = {};
      for (const row of rows) {
        if (row.metadata_value) {
          try {
            result[row.metadata_key] = JSON.parse(row.metadata_value);
          } catch {
            result[row.metadata_key] = row.metadata_value;
          }
        }
      }
      return result;
    } catch (error) {
      this.handleSqliteError(error, "getEntityMetadata", { entityType, entityId });
      throw error;
    }
  }

  /**
   * Update entity-level metadata (e.g., cleanup watermark)
   * Stores each key-value pair as a row in entity_metadata table with upsert semantics.
   */
  async setEntityMetadata(entityType: string, entityId: string, metadata: Record<string, unknown>): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      const stmt = db.prepare(`
        INSERT INTO entity_metadata (entity_type, entity_id, metadata_key, metadata_value, updated_at)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(entity_type, entity_id, metadata_key) DO UPDATE SET
          metadata_value = excluded.metadata_value,
          updated_at = excluded.updated_at
      `);

      const transaction = db.transaction(() => {
        for (const [key, value] of Object.entries(metadata)) {
          stmt.run(entityType, entityId, key, JSON.stringify(value), now);
        }
      });

      transaction();
    } catch (error) {
      this.handleSqliteError(error, "setEntityMetadata", { entityType, entityId });
      throw error;
    }
  }

  /**
   * Save multiple checkpoints in a single transaction
   */
  override async saveBatch(
    items: Array<{ id: string; data: Uint8Array; metadata: CheckpointStorageMetadata }>,
  ): Promise<void> {
    const db = this.getDb();
    const now = Date.now();
    const startTime = Date.now();

    try {
      // Phase 1: Extract all metrics and prepare data asynchronously (outside transaction)
      const preparedData = await Promise.all(
        items.map(async (item) => {
          const metrics = await this.extractMetrics(item.data);

          // Get compression config and compress synchronously
          const config = selectCompressionStrategy(item.data);
          const { compressed, algorithm } = compressBlobSync(item.data, config);

          return {
            item,
            metrics,
            compressed,
            algorithm,
          };
        }),
      );

      // Phase 2: Execute all inserts in a single transaction
      const transaction = db.transaction(() => {
        for (const prepared of preparedData) {
          const { item, metrics, compressed, algorithm } = prepared;

          db.prepare(`
            INSERT INTO checkpoint_metadata (
              id, entity_type, entity_id, timestamp, checkpoint_type,
              base_checkpoint_id, previous_checkpoint_id, message_count, variable_count,
              blob_size, blob_hash, tags, custom_fields, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              entity_type = excluded.entity_type,
              entity_id = excluded.entity_id,
              timestamp = excluded.timestamp,
              checkpoint_type = excluded.checkpoint_type,
              base_checkpoint_id = excluded.base_checkpoint_id,
              previous_checkpoint_id = excluded.previous_checkpoint_id,
              message_count = excluded.message_count,
              variable_count = excluded.variable_count,
              blob_size = excluded.blob_size,
              blob_hash = excluded.blob_hash,
              tags = excluded.tags,
              custom_fields = excluded.custom_fields,
              updated_at = excluded.updated_at
          `).run(
            item.id,
            item.metadata.entityType,
            item.metadata.entityId,
            item.metadata.timestamp,
            item.metadata.checkpointType ?? null,
            item.metadata.baseCheckpointId ?? null,
            item.metadata.previousCheckpointId ?? null,
            metrics.messageCount,
            metrics.variableCount,
            compressed.length,
            metrics.blobHash,
            item.metadata.tags ? JSON.stringify(item.metadata.tags) : null,
            item.metadata.customFields ? JSON.stringify(item.metadata.customFields) : null,
            now,
            now,
          );

          db.prepare(`
            INSERT INTO checkpoint_blob (checkpoint_id, blob_data, compressed, compression_algorithm)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(checkpoint_id) DO UPDATE SET
              blob_data = excluded.blob_data,
              compressed = excluded.compressed,
              compression_algorithm = excluded.compression_algorithm
          `).run(item.id, compressed, algorithm ? 1 : 0, algorithm || null);
        }
      });

      transaction();

      const elapsed = Date.now() - startTime;
      const totalSize = items.reduce((sum, item) => sum + item.data.length, 0);
      this.updateMetric('save', elapsed / items.length, totalSize);

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
      throw error;
    }
  }

  /**
   * Load multiple checkpoints efficiently
   */
  override async loadBatch(ids: string[]): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    const db = this.getDb();

    if (ids.length === 0) {
      return [];
    }

    try {
      // Use IN clause for efficient batch loading from blob table
      const placeholders = ids.map(() => '?').join(',');
      const stmt = db.prepare(`
        SELECT cb.checkpoint_id as id, cb.blob_data, cb.compressed, cb.compression_algorithm
        FROM checkpoint_blob cb
        WHERE cb.checkpoint_id IN (${placeholders})
      `);
      const rows = stmt.all(...ids) as Array<{
        id: string;
        blob_data: Buffer;
        compressed: number;
        compression_algorithm: string | null;
      }>;

      // Create a map for quick lookup
      const dataMap = new Map<string, Uint8Array>();
      for (const row of rows) {
        const buffer = row.blob_data;
        let data: Uint8Array;

        if (row.compressed && row.compression_algorithm) {
          // Decompress - zero-copy conversion before decompression
          const uncompressedBuffer = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
          data = decompressBlobSync(uncompressedBuffer, row.compression_algorithm);
        } else {
          // Zero-copy conversion: use shared ArrayBuffer to avoid memory duplication
          data = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
        }

        dataMap.set(row.id, data);
      }

      // Maintain order and handle missing items
      const results = ids.map(id => ({
        id,
        data: dataMap.get(id) || null,
      }));

      logger.debug("Batch load completed", {
        table: this.getTableName(),
        requested: ids.length,
        found: rows.length,
      });

      return results;
    } catch (error) {
      this.handleSqliteError(error, "loadBatch", { count: ids.length });
    }
  }

  /**
   * Delete multiple checkpoints in a single transaction
   */
  override async deleteBatch(ids: string[]): Promise<void> {
    const db = this.getDb();

    if (ids.length === 0) {
      return;
    }

    try {
      // Due to ON DELETE CASCADE, deleting from metadata will also delete from blob table
      const transaction = db.transaction(() => {
        const stmt = db.prepare(`DELETE FROM checkpoint_metadata WHERE id = ?`);
        for (const id of ids) {
          stmt.run(id);
        }
      });

      transaction();

      logger.debug("Batch delete completed", {
        table: this.getTableName(),
        count: ids.length,
      });
    } catch (error) {
      this.handleSqliteError(error, "deleteBatch", { count: ids.length });
    }
  }

  /**
   * List checkpoints for a specific entity with metadata
   * Optimized for entity-level queries using database indexes
   */
  async listByEntityWithMetadata(
    entityId: string,
    entityType: string,
    options?: { limit?: number; offset?: number }
  ): Promise<Array<{ id: string; metadata: CheckpointStorageMetadata }>> {
    const startTime = Date.now();
    const db = this.getDb();

    try {
      let sql = `
        SELECT id, entity_type, entity_id, timestamp, checkpoint_type,
               base_checkpoint_id, previous_checkpoint_id, message_count,
               variable_count, blob_size, blob_hash, tags, custom_fields
        FROM checkpoint_metadata
        WHERE entity_id = ? AND entity_type = ?
        ORDER BY timestamp DESC
      `;
      const params: unknown[] = [entityId, entityType];

      // Pagination with validation
      if (options?.limit !== undefined) {
        sql += " LIMIT ?";
        params.push(options.limit);
      }

      if (options?.offset !== undefined) {
        sql += " OFFSET ?";
        params.push(options.offset);
      }

      const stmt = db.prepare(sql);
      const rows = stmt.all(...params) as Array<{
        id: string;
        entity_type: string;
        entity_id: string;
        timestamp: number;
        checkpoint_type: string | null;
        base_checkpoint_id: string | null;
        previous_checkpoint_id: string | null;
        message_count: number | null;
        variable_count: number | null;
        blob_size: number | null;
        blob_hash: string | null;
        tags: string | null;
        custom_fields: string | null;
      }>;

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      return rows.map(row => ({
        id: row.id,
        metadata: {
          entityType: row.entity_type as CheckpointEntityType,
          entityId: row.entity_id,
          timestamp: row.timestamp,
          blobSize: row.blob_size ?? 0,
          customFields: {
            checkpointType: row.checkpoint_type,
            baseCheckpointId: row.base_checkpoint_id,
            previousCheckpointId: row.previous_checkpoint_id,
            messageCount: row.message_count,
            variableCount: row.variable_count,
            blobHash: row.blob_hash,
          },
          tags: row.tags ? JSON.parse(row.tags) : undefined,
        }
      }));
    } catch (error) {
      this.handleSqliteError(error, "listByEntityWithMetadata", { entityId, entityType });
      throw error;
    }
  }

  /**
   * Get the latest N checkpoints for a specific entity
   * Optimized for quick recovery scenarios
   */
  async getLatestByEntity(
    entityId: string,
    entityType: string,
    count: number = 1,
    includeData: boolean = false
  ): Promise<Array<{ id: string; metadata: CheckpointStorageMetadata; data?: Uint8Array }>> {
    const db = this.getDb();

    try {
      // First, get metadata
      const metadataSql = `
        SELECT id, entity_type, entity_id, timestamp, checkpoint_type,
               base_checkpoint_id, previous_checkpoint_id, message_count,
               variable_count, blob_size, blob_hash, tags, custom_fields
        FROM checkpoint_metadata
        WHERE entity_id = ? AND entity_type = ?
        ORDER BY timestamp DESC
        LIMIT ?
      `;
      
      const metadataStmt = db.prepare(metadataSql);
      const metadataRows = metadataStmt.all(entityId, entityType, count) as Array<{
        id: string;
        entity_type: string;
        entity_id: string;
        timestamp: number;
        checkpoint_type: string | null;
        base_checkpoint_id: string | null;
        previous_checkpoint_id: string | null;
        message_count: number | null;
        variable_count: number | null;
        blob_size: number | null;
        blob_hash: string | null;
        tags: string | null;
        custom_fields: string | null;
      }>;

      const results: Array<{ id: string; metadata: CheckpointStorageMetadata; data?: Uint8Array }> = [];

      for (const row of metadataRows) {
        const result: { id: string; metadata: CheckpointStorageMetadata; data?: Uint8Array } = {
          id: row.id,
          metadata: {
            entityType: row.entity_type as CheckpointEntityType,
            entityId: row.entity_id,
            timestamp: row.timestamp,
            blobSize: row.blob_size ?? 0,
            customFields: {
              checkpointType: row.checkpoint_type,
              baseCheckpointId: row.base_checkpoint_id,
              previousCheckpointId: row.previous_checkpoint_id,
              messageCount: row.message_count,
              variableCount: row.variable_count,
              blobHash: row.blob_hash,
            },
            tags: row.tags ? JSON.parse(row.tags) : undefined,
          }
        };

        // Optionally load BLOB data
        if (includeData) {
          const blobSql = `SELECT blob_data FROM checkpoint_blob WHERE checkpoint_id = ?`;
          const blobStmt = db.prepare(blobSql);
          const blobRow = blobStmt.get(row.id) as { blob_data: Buffer } | undefined;
          
          if (blobRow) {
            result.data = new Uint8Array(blobRow.blob_data);
          }
        }

        results.push(result);
      }

      return results;
    } catch (error) {
      this.handleSqliteError(error, "getLatestByEntity", { entityId, entityType });
      throw error;
    }
  }

  /**
   * Delete checkpoints for a specific entity with advanced options
   * Supports batch deletion with retention policies
   */
  async deleteByEntity(
    entityId: string,
    entityType: string,
    options?: {
      keepLatest?: number;
      olderThan?: number;
    }
  ): Promise<number> {
    const db = this.getDb();

    try {
      // Build query to find checkpoints to delete
      let selectSql = `
        SELECT id FROM checkpoint_metadata
        WHERE entity_id = ? AND entity_type = ?
      `;
      const params: unknown[] = [entityId, entityType];

      // Add time-based filter
      if (options?.olderThan) {
        selectSql += ` AND timestamp < ?`;
        params.push(options.olderThan);
      }

      // Order by timestamp descending
      selectSql += ` ORDER BY timestamp DESC`;

      // Skip the latest N checkpoints if specified
      if (options?.keepLatest && options.keepLatest > 0) {
        selectSql += ` LIMIT -1 OFFSET ?`;
        params.push(options.keepLatest);
      }

      // Get IDs to delete
      const selectStmt = db.prepare(selectSql);
      const rows = selectStmt.all(...params) as Array<{ id: string }>;
      const idsToDelete = rows.map(r => r.id);

      if (idsToDelete.length === 0) {
        return 0;
      }

      // Batch delete from both tables
      // SQLite supports DELETE with subquery
      const deleteSql = `
        DELETE FROM checkpoint_metadata
        WHERE id IN (${idsToDelete.map(() => '?').join(',')})
      `;
      
      const deleteStmt = db.prepare(deleteSql);
      const result = deleteStmt.run(...idsToDelete);

      logger.info('Deleted checkpoints by entity', {
        entityId,
        entityType,
        deletedCount: result.changes
      });

      return result.changes;
    } catch (error) {
      this.handleSqliteError(error, "deleteByEntity", { entityId, entityType });
      throw error;
    }
  }

  /**
   * Batch get latest checkpoints for multiple entities.
   * Uses a single query with ROW_NUMBER() window function for efficiency.
   *
   * @param entityIds Array of entity IDs
   * @param entityType Entity type filter
   * @param options Query options (limit per entity)
   * @returns Array of entity ID to latest checkpoints mapping
   */
  async listByEntitiesWithMetadata(
    entityIds: string[],
    entityType: string,
    options?: { limit?: number }
  ): Promise<Array<{
    entityId: string;
    checkpoints: Array<{ id: string; metadata: CheckpointStorageMetadata }>;
  }>> {
    const db = this.getDb();
    const limit = options?.limit ?? 1;

    if (entityIds.length === 0) {
      return [];
    }

    try {
      const placeholders = entityIds.map(() => '?').join(',');

      const sql = `
        WITH ranked_checkpoints AS (
          SELECT
            id,
            entity_id,
            entity_type,
            timestamp,
            checkpoint_type,
            base_checkpoint_id,
            previous_checkpoint_id,
            message_count,
            variable_count,
            blob_size,
            blob_hash,
            tags,
            custom_fields,
            ROW_NUMBER() OVER (
              PARTITION BY entity_id
              ORDER BY timestamp DESC
            ) as rn
          FROM checkpoint_metadata
          WHERE entity_id IN (${placeholders})
            AND entity_type = ?
        )
        SELECT *
        FROM ranked_checkpoints
        WHERE rn <= ?
        ORDER BY entity_id, timestamp DESC
      `;

      const params = [...entityIds, entityType, limit];
      const stmt = db.prepare(sql);
      const rows = stmt.all(...params) as Array<{
        id: string;
        entity_id: string;
        entity_type: string;
        timestamp: number;
        checkpoint_type: string | null;
        base_checkpoint_id: string | null;
        previous_checkpoint_id: string | null;
        message_count: number | null;
        variable_count: number | null;
        blob_size: number | null;
        blob_hash: string | null;
        tags: string | null;
        custom_fields: string | null;
        rn: number;
      }>;

      const resultMap = new Map<string, Array<{ id: string; metadata: CheckpointStorageMetadata }>>();

      for (const row of rows) {
        const checkpoint: { id: string; metadata: CheckpointStorageMetadata } = {
          id: row.id,
          metadata: {
            entityType: row.entity_type as CheckpointEntityType,
            entityId: row.entity_id,
            timestamp: row.timestamp,
            blobSize: row.blob_size ?? 0,
            customFields: {
              checkpointType: row.checkpoint_type,
              baseCheckpointId: row.base_checkpoint_id,
              previousCheckpointId: row.previous_checkpoint_id,
              messageCount: row.message_count,
              variableCount: row.variable_count,
              blobHash: row.blob_hash,
            },
            tags: row.tags ? JSON.parse(row.tags) : undefined,
          },
        };

        const existing = resultMap.get(row.entity_id) || [];
        existing.push(checkpoint);
        resultMap.set(row.entity_id, existing);
      }

      const results: Array<{ entityId: string; checkpoints: Array<{ id: string; metadata: CheckpointStorageMetadata }> }> = [];

      for (const entityId of entityIds) {
        results.push({
          entityId,
          checkpoints: resultMap.get(entityId) || [],
        });
      }

      return results;
    } catch (error) {
      this.handleSqliteError(error, "listByEntitiesWithMetadata", { entityType, entityCount: entityIds.length });
      throw error;
    }
  }
}
