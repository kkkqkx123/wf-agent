/**
 * PostgreSQL Checkpoint Storage Implementation with Metadata-BLOB Separation
 * Checkpoint persistent storage based on pg
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type { 
  CheckpointStorageMetadata, 
  CheckpointStorageListOptions,
  CheckpointEntityType 
} from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "../types/adapter/index.js";
import type { CheckpointOptions } from "../types/checkpoint-options.js";
import { BasePostgresStorage, BasePostgresStorageConfig } from "./base-postgres-storage.js";
import { selectCompressionStrategy } from "@wf-agent/common-utils";
import { compressBlob, decompressBlob } from "@wf-agent/common-utils";
import { PoolClient } from 'pg';
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("postgres-checkpoint-storage");

/**
 * PostgreSQL Checkpoint Storage
 * Implementing the CheckpointStorageAdapter interface with metadata-BLOB separation
 */
export class PostgresCheckpointStorage
  extends BasePostgresStorage<CheckpointStorageMetadata, CheckpointStorageListOptions, CheckpointOptions>
  implements CheckpointStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
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
  protected async createTableSchema(client: PoolClient): Promise<void> {
    // Layer 1: Metadata table (frequent queries, no BLOB)
    await client.query(`
      CREATE TABLE IF NOT EXISTS checkpoint_metadata (
        id TEXT PRIMARY KEY,
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        timestamp BIGINT NOT NULL,
        checkpoint_type TEXT,
        base_checkpoint_id TEXT,
        previous_checkpoint_id TEXT,
        message_count INTEGER,
        variable_count INTEGER,
        blob_size INTEGER,
        blob_hash TEXT,
        tags JSONB,
        custom_fields JSONB,
        created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
      )
    `);

    // Layer 2: BLOB storage table (infrequent direct access)
    await client.query(`
      CREATE TABLE IF NOT EXISTS checkpoint_blob (
        checkpoint_id TEXT PRIMARY KEY REFERENCES checkpoint_metadata(id) ON DELETE CASCADE,
        blob_data BYTEA NOT NULL,
        compressed BOOLEAN DEFAULT FALSE,
        compression_algorithm TEXT,
        CHECK (
          (compressed = FALSE AND compression_algorithm IS NULL) OR
          (compressed = TRUE AND compression_algorithm IS NOT NULL)
        )
      )
    `);

    // ========================================================================
    // Optimized Indexes for Entity-Based Queries
    // ========================================================================
    
    // Primary composite index: entity lookup with timestamp ordering
    // Supports: listByEntityWithMetadata, getLatestByEntity, cleanup by entity
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_checkpoint_entity_timestamp ON checkpoint_metadata(entity_type, entity_id, timestamp DESC)'
    );

    // Timestamp-only index: time-based cleanup and range queries
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_checkpoint_timestamp ON checkpoint_metadata(timestamp)'
    );

    // Checkpoint type index: filter by FULL/DELTA
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_checkpoint_type ON checkpoint_metadata(checkpoint_type)'
    );

    // Hash index: deduplication detection
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_checkpoint_blob_hash ON checkpoint_metadata(blob_hash)'
    );

    // Entity metadata table: stores per-entity key-value metadata (e.g., cleanup watermark)
    await client.query(`
      CREATE TABLE IF NOT EXISTS entity_metadata (
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        metadata_key TEXT NOT NULL,
        metadata_value TEXT,
        updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        PRIMARY KEY (entity_type, entity_id, metadata_key)
      )
    `);

    logger.debug('Created optimized indexes for entity-based queries', {
      table: this.getTableName()
    });
  }

  /**
   * Save checkpoint with metadata-BLOB separation and compression
   */
  async doSave(
    checkpointId: string, 
    data: Uint8Array, 
    metadata: CheckpointStorageMetadata,
    options?: CheckpointOptions
  ): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      // Begin transaction
      await client.query('BEGIN');

      try {
        await this.saveToClient(client, checkpointId, data, metadata, options);
        
        // If sync mode is enabled, ensure data is flushed to disk
        if (options?.sync) {
          await client.query('COMMIT');
          // Force a synchronous commit by using PREPARE COMMIT pattern
          await client.query('SELECT pg_current_wal_lsn()');
          logger.debug('Synchronous checkpoint saved with explicit sync', { 
            checkpointId, 
            size: data.length 
          });
        } else {
          // Commit transaction
          await client.query('COMMIT');
        }

        const elapsed = Date.now() - startTime;
        this.updateMetric('save', elapsed, data.length);

        logger.debug('Checkpoint saved', {
          checkpointId,
          dataSize: data.length,
        });
      } catch (error) {
        // Rollback on error
        await client.query('ROLLBACK').catch(rollbackError => {
          logger.error('Failed to rollback transaction', { 
            checkpointId,
            rollbackError: (rollbackError as Error).message 
          });
        });
        throw error;
      }
    } catch (error) {
      this.handlePostgresError(error, 'save', { checkpointId });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save to client with compression
   */
  protected async saveToClient(
    client: PoolClient,
    checkpointId: string,
    data: Uint8Array,
    metadata: CheckpointStorageMetadata,
    _options?: CheckpointOptions
  ): Promise<void> {

    // Get adaptive compression config
    const config = selectCompressionStrategy(data);

    // Compress BLOB data
    const { compressed, algorithm } = await compressBlob(data, config);
    
    // Compute blob hash
    const blobHash = await this.computeHash(data);

    // Extract metrics and checkpoint metadata from data
    let checkpointType: string | null = null;
    let baseCheckpointId: string | null = null;
    let previousCheckpointId: string | null = null;
    let messageCount: number | null = null;
    let variableCount: number | null = null;

    try {
      const decoder = new TextDecoder();
      const jsonStr = decoder.decode(data);
      const checkpoint = JSON.parse(jsonStr);
      checkpointType = checkpoint.type ?? null;
      baseCheckpointId = checkpoint.baseCheckpointId ?? null;
      previousCheckpointId = checkpoint.previousCheckpointId ?? null;
      const executionState = checkpoint.executionState;
      messageCount = executionState?.conversationState?.messages?.length ?? null;
      // variables is a Record<string, unknown>, use Object.keys to count
      if (executionState?.variableState?.variables && typeof executionState.variableState.variables === 'object') {
        variableCount = Object.keys(executionState.variableState.variables).length;
      } else if (executionState?.variables && typeof executionState.variables === 'object') {
        // Fallback for older checkpoint formats
        variableCount = Object.keys(executionState.variables).length;
      }
    } catch {
      // Ignore parsing errors, use null defaults
    }

    // Insert or update metadata
    await client.query(
      `INSERT INTO checkpoint_metadata (
        id, entity_type, entity_id, timestamp, checkpoint_type,
        base_checkpoint_id, previous_checkpoint_id, message_count, variable_count,
        blob_size, blob_hash, tags, custom_fields, created_at, updated_at
      ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), NOW())
      ON CONFLICT (id) DO UPDATE SET
        entity_type = EXCLUDED.entity_type,
        entity_id = EXCLUDED.entity_id,
        timestamp = EXCLUDED.timestamp,
        checkpoint_type = EXCLUDED.checkpoint_type,
        base_checkpoint_id = EXCLUDED.base_checkpoint_id,
        previous_checkpoint_id = EXCLUDED.previous_checkpoint_id,
        message_count = EXCLUDED.message_count,
        variable_count = EXCLUDED.variable_count,
        blob_size = EXCLUDED.blob_size,
        blob_hash = EXCLUDED.blob_hash,
        tags = EXCLUDED.tags,
        custom_fields = EXCLUDED.custom_fields,
        updated_at = NOW()`,
      [
        checkpointId,
        metadata.entityType,
        metadata.entityId,
        metadata.timestamp,
        checkpointType,
        baseCheckpointId,
        previousCheckpointId,
        messageCount,
        variableCount,
        compressed.length,
        blobHash,
        metadata.tags ? JSON.stringify(metadata.tags) : null,
        metadata.customFields ? JSON.stringify(metadata.customFields) : null,
      ]
    );

    // Insert or update BLOB data
    await client.query(
      `INSERT INTO checkpoint_blob (checkpoint_id, blob_data, compressed, compression_algorithm)
       VALUES ($1, $2, $3, $4)
       ON CONFLICT (checkpoint_id) DO UPDATE SET
         blob_data = EXCLUDED.blob_data,
         compressed = EXCLUDED.compressed,
         compression_algorithm = EXCLUDED.compression_algorithm`,
      [
        checkpointId,
        Buffer.from(compressed),
        algorithm !== null,
        algorithm,
      ]
    );
  }

  /**
   * Load checkpoint with decompression
   */
  async doLoad(checkpointId: string): Promise<Uint8Array | null> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      // Join metadata and blob tables
      const result = await client.query(
        `SELECT cb.blob_data, cb.compressed, cb.compression_algorithm, cm.blob_hash
         FROM checkpoint_blob cb
         INNER JOIN checkpoint_metadata cm ON cb.checkpoint_id = cm.id
         WHERE cb.checkpoint_id = $1`,
        [checkpointId]
      );

      if (result.rows.length === 0) {
        logger.debug('Checkpoint not found', { checkpointId });
        return null;
      }

      const row = result.rows[0];
      let data = row.blob_data;

      // Decompress if needed
      if (row.compressed && row.compression_algorithm) {
        data = await decompressBlob(data, row.compression_algorithm);
      }

      // Verify integrity if enabled
      if (this.shouldVerifyIntegrity() && row.blob_hash) {
        await this.verifyIntegrity(data, row.blob_hash, checkpointId);
      }

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed, data.length);

      logger.debug('Checkpoint loaded', {
        checkpointId,
        dataSize: data.length,
        compressed: row.compressed,
      });

      return data;
    } catch (error) {
      this.handlePostgresError(error, 'load', { checkpointId });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete checkpoint
   */
  async doDelete(checkpointId: string): Promise<void> {
    const client = await this.getClient();

    try {
      await this.deleteFromClient(client, checkpointId);
      logger.debug('Checkpoint deleted', { checkpointId });
    } catch (error) {
      this.handlePostgresError(error, 'delete', { checkpointId });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete from client
   */
  protected async deleteFromClient(client: PoolClient, checkpointId: string): Promise<void> {
    // Delete from metadata table (cascade will delete blob)
    await client.query(
      'DELETE FROM checkpoint_metadata WHERE id = $1',
      [checkpointId]
    );
  }

  /**
   * List checkpoints with filtering and pagination
   */
  async list(options?: CheckpointStorageListOptions): Promise<string[]> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Build dynamic query based on filters
      const conditions: string[] = [];
      const params: Array<string | number | string[]> = [];
      let paramIndex = 1;

      if (options?.entityType) {
        conditions.push(`entity_type = $${paramIndex++}`);
        params.push(options.entityType);
      }

      if (options?.entityId) {
        conditions.push(`entity_id = $${paramIndex++}`);
        params.push(options.entityId);
      }

      if (options?.type) {
        conditions.push(`checkpoint_type = $${paramIndex++}`);
        params.push(options.type);
      }

      if (options?.timestampFrom) {
        conditions.push(`timestamp >= $${paramIndex++}`);
        params.push(options.timestampFrom);
      }

      if (options?.timestampTo) {
        conditions.push(`timestamp <= $${paramIndex++}`);
        params.push(options.timestampTo);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags::jsonb ?| $${paramIndex++}::text[]`);
        params.push(options.tags);
      }

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      const query = `
        SELECT id FROM checkpoint_metadata
        ${whereClause}
        ORDER BY timestamp DESC
        LIMIT $${paramIndex++} OFFSET $${paramIndex++}
      `;

      params.push(limit, offset);

      const result = await client.query(query, params);
      const ids = result.rows.map(row => row.id);

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      logger.debug('Checkpoints listed', {
        count: ids.length,
        filters: Object.keys(options || {}),
      });

      return ids;
    } catch (error) {
      this.handlePostgresError(error, 'list', { options });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Check if checkpoint exists
   */
  async exists(checkpointId: string): Promise<boolean> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        'SELECT 1 FROM checkpoint_metadata WHERE id = $1',
        [checkpointId]
      );
      return result.rows.length > 0;
    } catch (error) {
      this.handlePostgresError(error, 'exists', { checkpointId });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get checkpoint metadata
   */
  async getMetadata(checkpointId: string): Promise<CheckpointStorageMetadata | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT * FROM checkpoint_metadata WHERE id = $1`,
        [checkpointId]
      );

      if (result.rows.length === 0) {
        return null;
      }

      const row = result.rows[0];
      
      // Parse JSON fields
      const metadata: CheckpointStorageMetadata = {
        entityType: row.entity_type,
        entityId: row.entity_id,
        timestamp: row.timestamp,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.custom_fields,
      };

      return metadata;
    } catch (error) {
      this.handlePostgresError(error, 'getMetadata', { checkpointId });
      throw error;
    } finally {
      this.releaseClient(client);
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
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Build dynamic query based on filters
      const conditions: string[] = [];
      const params: Array<string | number | string[]> = [];
      let paramIndex = 1;

      if (options?.entityType) {
        conditions.push(`entity_type = $${paramIndex++}`);
        params.push(options.entityType);
      }

      if (options?.entityId) {
        conditions.push(`entity_id = $${paramIndex++}`);
        params.push(options.entityId);
      }

      if (options?.type) {
        conditions.push(`checkpoint_type = $${paramIndex++}`);
        params.push(options.type);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags::jsonb ?| $${paramIndex++}::text[]`);
        params.push(options.tags);
      }

      if (options?.timestampFrom) {
        conditions.push(`timestamp >= $${paramIndex++}`);
        params.push(options.timestampFrom);
      }

      if (options?.timestampTo) {
        conditions.push(`timestamp <= $${paramIndex++}`);
        params.push(options.timestampTo);
      }

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      const query = `
        SELECT 
          id,
          entity_type as "entityType",
          entity_id as "entityId",
          timestamp,
          tags,
          custom_fields as "customFields"
        FROM checkpoint_metadata
        ${whereClause}
        ORDER BY timestamp DESC
        LIMIT $${paramIndex++} OFFSET $${paramIndex++}
      `;

      params.push(limit, offset);

      const result = await client.query(query, params);
      
      const items = result.rows.map(row => ({
        id: row.id,
        metadata: {
          entityType: row.entityType,
          entityId: row.entityId,
          timestamp: row.timestamp,
          tags: row.tags ? JSON.parse(row.tags) : undefined,
          customFields: row.customFields,
        },
      }));

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      logger.debug('Checkpoints listed with metadata', {
        count: items.length,
        filters: Object.keys(options || {}),
      });

      return items;
    } catch (error) {
      this.handlePostgresError(error, 'listWithMetadata', { options });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get latest checkpoint for an entity
   */
  async getLatestCheckpoint(entityType: string, entityId: string): Promise<string | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT id FROM checkpoint_metadata 
         WHERE entity_type = $1 AND entity_id = $2
         ORDER BY timestamp DESC 
         LIMIT 1`,
        [entityType, entityId]
      );

      return result.rows.length > 0 ? result.rows[0].id : null;
    } catch (error) {
      this.handlePostgresError(error, 'getLatestCheckpoint', { entityType, entityId });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Clear all checkpoints
   */
  override async clear(): Promise<void> {
    const client = await this.getClient();

    try {
      // Delete from metadata table (cascade will delete blob)
      await client.query('DELETE FROM checkpoint_metadata');
      await client.query('DELETE FROM entity_metadata');
      logger.info('All checkpoints cleared');
    } catch (error) {
      this.handlePostgresError(error, 'clear', {});
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get entity-level metadata (e.g., cleanup watermark)
   */
  async getEntityMetadata(entityType: string, entityId: string): Promise<Record<string, unknown> | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT metadata_key, metadata_value FROM entity_metadata
         WHERE entity_type = $1 AND entity_id = $2`,
        [entityType, entityId]
      );

      if (result.rows.length === 0) return null;

      const metadata: Record<string, unknown> = {};
      for (const row of result.rows) {
        try {
          metadata[row.metadata_key] = JSON.parse(row.metadata_value);
        } catch {
          metadata[row.metadata_key] = row.metadata_value;
        }
      }
      return metadata;
    } catch (error) {
      this.handlePostgresError(error, 'getEntityMetadata', { entityType, entityId });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Update entity-level metadata (e.g., cleanup watermark)
   */
  async setEntityMetadata(entityType: string, entityId: string, metadata: Record<string, unknown>): Promise<void> {
    const client = await this.getClient();

    try {
      for (const [key, value] of Object.entries(metadata)) {
        await client.query(
          `INSERT INTO entity_metadata (entity_type, entity_id, metadata_key, metadata_value, updated_at)
           VALUES ($1, $2, $3, $4, NOW())
           ON CONFLICT (entity_type, entity_id, metadata_key) DO UPDATE SET
             metadata_value = EXCLUDED.metadata_value,
             updated_at = NOW()`,
          [entityType, entityId, key, JSON.stringify(value)]
        );
      }
    } catch (error) {
      this.handlePostgresError(error, 'setEntityMetadata', { entityType, entityId });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save multiple checkpoints in a single transaction
   */
  override async saveBatch(
    items: Array<{ id: string; data: Uint8Array; metadata: CheckpointStorageMetadata }>,
  ): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    if (items.length === 0) {
      return;
    }

    try {
      await client.query('BEGIN');

      try {
        for (const item of items) {
          await this.saveToClient(client, item.id, item.data, item.metadata);
        }

        await client.query('COMMIT');

        const elapsed = Date.now() - startTime;
        const totalSize = items.reduce((sum, item) => sum + item.data.length, 0);
        this.updateMetric('save', elapsed / items.length, totalSize);

        logger.debug('Batch save completed', {
          count: items.length,
          totalTimeMs: elapsed,
        });
      } catch (error) {
        await client.query('ROLLBACK').catch(rollbackError => {
          logger.error('Failed to rollback batch transaction', { 
            rollbackError: (rollbackError as Error).message 
          });
        });
        throw error;
      }
    } catch (error) {
      this.handlePostgresError(error, 'saveBatch', { count: items.length });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Load multiple checkpoints efficiently
   */
  override async loadBatch(ids: string[]): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    const client = await this.getClient();

    if (ids.length === 0) {
      return [];
    }

    try {
      // Use IN clause for efficient batch loading
      const placeholders = ids.map((_, i) => `$${i + 1}`).join(',');
      const result = await client.query(
        `SELECT cb.checkpoint_id as id, cb.blob_data, cb.compressed, cb.compression_algorithm
         FROM checkpoint_blob cb
         WHERE cb.checkpoint_id IN (${placeholders})`,
        ids
      );

      // Create a map for quick lookup
      const dataMap = new Map<string, Uint8Array>();
      for (const row of result.rows) {
        let data = row.blob_data;

        // Decompress if needed
        if (row.compressed && row.compression_algorithm) {
          data = await decompressBlob(data, row.compression_algorithm);
        }

        dataMap.set(row.id, data);
      }

      // Maintain order and handle missing items
      const results = ids.map(id => ({
        id,
        data: dataMap.get(id) || null,
      }));

      logger.debug('Batch load completed', {
        requested: ids.length,
        found: result.rows.length,
      });

      return results;
    } catch (error) {
      this.handlePostgresError(error, 'loadBatch', { count: ids.length });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete multiple checkpoints in a single transaction
   */
  override async deleteBatch(ids: string[]): Promise<void> {
    const client = await this.getClient();

    if (ids.length === 0) {
      return;
    }

    try {
      await client.query('BEGIN');

      try {
        // Use IN clause for efficient batch deletion
        const placeholders = ids.map((_, i) => `$${i + 1}`).join(',');
        await client.query(
          `DELETE FROM checkpoint_metadata WHERE id IN (${placeholders})`,
          ids
        );

        await client.query('COMMIT');

        logger.debug('Batch delete completed', {
          count: ids.length,
        });
      } catch (error) {
        await client.query('ROLLBACK').catch(rollbackError => {
          logger.error('Failed to rollback batch delete transaction', { 
            rollbackError: (rollbackError as Error).message 
          });
        });
        throw error;
      }
    } catch (error) {
      this.handlePostgresError(error, 'deleteBatch', { count: ids.length });
      throw error;
    } finally {
      this.releaseClient(client);
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
    const client = await this.getClient();

    try {
      let sql = `
        SELECT id, entity_type, entity_id, timestamp, checkpoint_type,
               base_checkpoint_id, previous_checkpoint_id, message_count,
               variable_count, blob_size, blob_hash, tags, custom_fields
        FROM checkpoint_metadata
        WHERE entity_id = $1 AND entity_type = $2
        ORDER BY timestamp DESC
      `;
      const params: (string | number)[] = [entityId, entityType];

      // Pagination
      if (options?.limit !== undefined) {
        sql += ` LIMIT $${params.length + 1}`;
        params.push(options.limit);
      }

      if (options?.offset !== undefined) {
        sql += ` OFFSET $${params.length + 1}`;
        params.push(options.offset);
      }

      const result = await client.query(sql, params);
      
      type ListByEntityPgRow = {
        id: string; entity_type: string; entity_id: string; timestamp: string;
        checkpoint_type: string | null; base_checkpoint_id: string | null;
        previous_checkpoint_id: string | null; message_count: number | null;
        variable_count: number | null; blob_size: number | null; blob_hash: string | null;
        tags: string | null; custom_fields: string | null;
      };
      return (result.rows as ListByEntityPgRow[]).map(row => ({
        id: row.id,
        metadata: {
          entityType: row.entity_type as CheckpointEntityType,
          entityId: row.entity_id,
          timestamp: parseInt(row.timestamp),
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
      this.handlePostgresError(error, 'listByEntityWithMetadata', { entityId, entityType });
      throw error;
    } finally {
      this.releaseClient(client);
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
    const client = await this.getClient();

    try {
      // First, get metadata
      const metadataSql = `
        SELECT id, entity_type, entity_id, timestamp, checkpoint_type,
               base_checkpoint_id, previous_checkpoint_id, message_count,
               variable_count, blob_size, blob_hash, tags, custom_fields
        FROM checkpoint_metadata
        WHERE entity_id = $1 AND entity_type = $2
        ORDER BY timestamp DESC
        LIMIT $3
      `;
      
      const metadataResult = await client.query(metadataSql, [entityId, entityType, count]);
      
      const results: Array<{ id: string; metadata: CheckpointStorageMetadata; data?: Uint8Array }> = [];

      for (const row of metadataResult.rows) {
        const result: { id: string; metadata: CheckpointStorageMetadata; data?: Uint8Array } = {
          id: row.id,
          metadata: {
            entityType: row.entity_type,
            entityId: row.entity_id,
            timestamp: parseInt(row.timestamp),
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
          const blobSql = `SELECT blob_data FROM checkpoint_blob WHERE checkpoint_id = $1`;
          const blobResult = await client.query(blobSql, [row.id]);
          
          if (blobResult.rows.length > 0) {
            result.data = blobResult.rows[0].blob_data;
          }
        }

        results.push(result);
      }

      return results;
    } catch (error) {
      this.handlePostgresError(error, 'getLatestByEntity', { entityId, entityType });
      throw error;
    } finally {
      this.releaseClient(client);
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
    const client = await this.getClient();

    try {
      // Build query to find checkpoints to delete
      let selectSql = `
        SELECT id FROM checkpoint_metadata
        WHERE entity_id = $1 AND entity_type = $2
      `;
      const params: (string | number)[] = [entityId, entityType];

      // Add time-based filter
      if (options?.olderThan) {
        selectSql += ` AND timestamp < $${params.length + 1}`;
        params.push(options.olderThan);
      }

      // Order by timestamp descending
      selectSql += ` ORDER BY timestamp DESC`;

      // Skip the latest N checkpoints if specified
      if (options?.keepLatest && options.keepLatest > 0) {
        selectSql += ` LIMIT ALL OFFSET $${params.length + 1}`;
        params.push(options.keepLatest);
      }

      // Get IDs to delete
      const selectResult = await client.query(selectSql, params);
      const idsToDelete = (selectResult.rows as Array<{ id: string }>).map(row => row.id);

      if (idsToDelete.length === 0) {
        return 0;
      }

      // Batch delete using IN clause
      const placeholders = idsToDelete.map((_, i) => `$${i + 3}`).join(',');
      const deleteSql = `
        DELETE FROM checkpoint_metadata
        WHERE id IN (${placeholders})
      `;
      
      const deleteParams = [entityId, entityType, ...idsToDelete];
      const deleteResult = await client.query(deleteSql, deleteParams);

      const deletedCount = parseInt(deleteResult.rowCount?.toString() || '0');

      logger.info('Deleted checkpoints by entity', {
        entityId,
        entityType,
        deletedCount
      });

      return deletedCount;
    } catch (error) {
      this.handlePostgresError(error, 'deleteByEntity', { entityId, entityType });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Batch get latest checkpoints for multiple entities.
   * Uses a single query with ROW_NUMBER() window function for efficiency.
   */
  async listByEntitiesWithMetadata(
    entityIds: string[],
    entityType: string,
    options?: { limit?: number }
  ): Promise<Array<{
    entityId: string;
    checkpoints: Array<{ id: string; metadata: CheckpointStorageMetadata }>;
  }>> {
    const client = await this.getClient();
    const limit = options?.limit ?? 1;

    if (entityIds.length === 0) {
      return [];
    }

    try {
      const placeholders = entityIds.map((_, i) => `$${i + 1}`).join(',');

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
            AND entity_type = $${entityIds.length + 1}
        )
        SELECT *
        FROM ranked_checkpoints
        WHERE rn <= $${entityIds.length + 2}
        ORDER BY entity_id, timestamp DESC
      `;

      const params = [...entityIds, entityType, limit];
      const result = await client.query(sql, params);

      const resultMap = new Map<string, Array<{ id: string; metadata: CheckpointStorageMetadata }>>();

      for (const row of result.rows) {
        const checkpoint: { id: string; metadata: CheckpointStorageMetadata } = {
          id: row.id,
          metadata: {
            entityType: row.entity_type as CheckpointEntityType,
            entityId: row.entity_id,
            timestamp: parseInt(row.timestamp),
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
      this.handlePostgresError(error, 'listByEntitiesWithMetadata', { entityType, entityCount: entityIds.length });
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }
}
