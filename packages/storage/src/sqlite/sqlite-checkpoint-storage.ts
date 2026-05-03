/**
 * SQLite Checkpoint Storage Implementation with Metadata-BLOB Separation
 * Checkpoint persistent storage based on better-sqlite3
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type { CheckpointStorageMetadata, CheckpointStorageListOptions } from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "../types/adapter/index.js";
import { BaseSqliteStorage, BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
import { selectCompressionStrategy } from "../compression/adaptive-compression.js";
import { compressBlob, decompressBlob, compressBlobSync, decompressBlobSync } from "../compression/compressor.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("sqlite-checkpoint-storage");

/**
 * SQLite Checkpoint Storage
 * Implementing the CheckpointStorageAdapter interface with metadata-BLOB separation
 */
export class SqliteCheckpointStorage
  extends BaseSqliteStorage<CheckpointStorageMetadata>
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
  protected getBlobTableName(): string {
    return "checkpoint_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected createTableSchema(): void {
    const db = this.getDb();

    // Layer 1: Metadata table (frequent queries, no BLOB)
    db.exec(`
      CREATE TABLE IF NOT EXISTS checkpoint_metadata (
        id TEXT PRIMARY KEY,
        execution_id TEXT NOT NULL,
        workflow_id TEXT NOT NULL,
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
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
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

    // Create indexes for optimized queries
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_meta_execution_id ON checkpoint_metadata(execution_id)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_meta_workflow_id ON checkpoint_metadata(workflow_id)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_meta_timestamp ON checkpoint_metadata(timestamp)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_meta_type ON checkpoint_metadata(checkpoint_type)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_meta_execution_timestamp ON checkpoint_metadata(execution_id, timestamp)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_checkpoint_meta_workflow_timestamp ON checkpoint_metadata(workflow_id, timestamp)`,
    );
  }

  /**
   * Extract metrics from checkpoint data for metadata storage
   */
  private async extractMetrics(data: Uint8Array): Promise<{
    messageCount: number;
    variableCount: number;
    blobHash: string;
  }> {
    try {
      // Parse the checkpoint data to extract metrics
      const decoder = new TextDecoder();
      const jsonStr = decoder.decode(data);
      const checkpoint = JSON.parse(jsonStr);

      const executionState = checkpoint.executionState;
      const messageCount = executionState?.conversationState?.messages?.length ?? 0;
      const variableCount = executionState?.variables?.length ?? 0;

      // Simple hash for deduplication detection
      const blobHash = await this.computeHash(data);

      return { messageCount, variableCount, blobHash };
    } catch {
      // If parsing fails, return default values
      return {
        messageCount: 0,
        variableCount: 0,
        blobHash: await this.computeHash(data),
      };
    }
  }

  /**
   * Save checkpoint with metadata-BLOB separation and compression
   */
  async save(id: string, data: Uint8Array, metadata: CheckpointStorageMetadata): Promise<void> {
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
          id, execution_id, workflow_id, timestamp, checkpoint_type,
          base_checkpoint_id, previous_checkpoint_id, message_count, variable_count,
          blob_size, blob_hash, tags, custom_fields, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          execution_id = excluded.execution_id,
          workflow_id = excluded.workflow_id,
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
        try {
          // Extract checkpoint type and IDs if available
          let checkpointType: string | null = null;
          let baseCheckpointId: string | null = null;
          let previousCheckpointId: string | null = null;

          try {
            const decoder = new TextDecoder();
            const jsonStr = decoder.decode(data);
            const checkpoint = JSON.parse(jsonStr);
            checkpointType = checkpoint.type ?? null;
            baseCheckpointId = checkpoint.baseCheckpointId ?? null;
            previousCheckpointId = checkpoint.previousCheckpointId ?? null;
          } catch {
            // Ignore parsing errors
          }

          insertMetadata.run(
            id,
            metadata.executionId,
            metadata.workflowId,
            metadata.timestamp,
            checkpointType,
            baseCheckpointId,
            previousCheckpointId,
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
        } catch (error) {
          logger.error("Transaction failed during checkpoint save, rolling back", { 
            id, 
            error: (error as Error).message 
          });
          throw error; // Transaction will automatically rollback
        }
      })();

      // Track metrics
      const elapsed = Date.now() - startTime;
      this.updateMetric('save', elapsed, compressed.length);
    } catch (error) {
      this.handleSqliteError(error, "save", { id });
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
      if (options?.executionId) {
        conditions.push("execution_id = ?");
        params.push(options.executionId);
      }

      if (options?.workflowId) {
        conditions.push("workflow_id = ?");
        params.push(options.workflowId);
      }

      if (options?.tags && options.tags.length > 0) {
        // Use parameterized query for tags to prevent SQL injection
        const tagPattern = `%${options.tags[0]}%`;
        conditions.push("tags LIKE ?");
        params.push(tagPattern);
      }

      if (conditions.length > 0) {
        sql += " WHERE " + conditions.join(" AND ");
      }

      // Sort by timestamp descending
      sql += " ORDER BY timestamp DESC";

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
          execution_id as "executionId",
          workflow_id as "workflowId",
          timestamp,
          tags,
          custom_fields as "customFields"
        FROM checkpoint_metadata WHERE id = ?
      `);
      const row = stmt.get(id) as
        | {
            executionId: string;
            workflowId: string;
            timestamp: number;
            tags: string | null;
            customFields: string | null;
          }
        | undefined;

      if (!row) {
        return null;
      }

      return {
        executionId: row.executionId,
        workflowId: row.workflowId,
        timestamp: row.timestamp,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.customFields ? JSON.parse(row.customFields) : undefined,
      };
    } catch (error) {
      this.handleSqliteError(error, "getMetadata", { id });
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
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
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

    // Prepare statements
    const insertMetadata = db.prepare(`
      INSERT INTO checkpoint_metadata (
        id, execution_id, workflow_id, timestamp, checkpoint_type,
        base_checkpoint_id, previous_checkpoint_id, message_count, variable_count,
        blob_size, blob_hash, tags, custom_fields, created_at, updated_at
      )
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        execution_id = excluded.execution_id,
        workflow_id = excluded.workflow_id,
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

    await this.saveBatchWithCustomLogic(items, async (db, item) => {
      const metrics = await this.extractMetrics(item.data);
      
      let checkpointType: string | null = null;
      let baseCheckpointId: string | null = null;
      let previousCheckpointId: string | null = null;

      try {
        const decoder = new TextDecoder();
        const jsonStr = decoder.decode(item.data);
        const checkpoint = JSON.parse(jsonStr);
        checkpointType = checkpoint.type ?? null;
        baseCheckpointId = checkpoint.baseCheckpointId ?? null;
        previousCheckpointId = checkpoint.previousCheckpointId ?? null;
      } catch {
        // Ignore parsing errors
      }

      // Get adaptive compression config for batch operation
      // Get compression config for each item
      const config = selectCompressionStrategy(item.data);

      // Compress BLOB data synchronously for batch operation
      const { compressed, algorithm } = compressBlobSync(item.data, config);

      insertMetadata.run(
        item.id,
        item.metadata.executionId,
        item.metadata.workflowId,
        item.metadata.timestamp,
        checkpointType,
        baseCheckpointId,
        previousCheckpointId,
        metrics.messageCount,
        metrics.variableCount,
        compressed.length,
        metrics.blobHash,
        item.metadata.tags ? JSON.stringify(item.metadata.tags) : null,
        item.metadata.customFields ? JSON.stringify(item.metadata.customFields) : null,
        now,
        now,
      );

      insertBlob.run(item.id, compressed, algorithm ? 1 : 0, algorithm || null);
    });
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
}
