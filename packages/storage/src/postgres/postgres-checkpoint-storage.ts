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
  CheckpointStorageListOptions 
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
  extends BasePostgresStorage<CheckpointStorageMetadata>
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
   */
  protected async createTableSchema(client: PoolClient): Promise<void> {
    // Layer 1: Metadata table (frequent queries, no BLOB)
    await client.query(`
      CREATE TABLE IF NOT EXISTS checkpoint_metadata (
        id TEXT PRIMARY KEY,
        execution_id TEXT,
        workflow_id TEXT,
        entity_type TEXT,
        entity_id TEXT,
        timestamp BIGINT NOT NULL,
        checkpoint_type TEXT,
        base_checkpoint_id TEXT,
        previous_checkpoint_id TEXT,
        message_count INTEGER,
        variable_count INTEGER,
        blob_size INTEGER,
        blob_hash TEXT,
        tags TEXT,
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

    // Create indexes for optimized queries
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_execution ON checkpoint_metadata(execution_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_workflow ON checkpoint_metadata(workflow_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_entity_type ON checkpoint_metadata(entity_type)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_entity_id ON checkpoint_metadata(entity_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_timestamp ON checkpoint_metadata(timestamp)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_type ON checkpoint_metadata(checkpoint_type)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_exec_ts ON checkpoint_metadata(execution_id, timestamp)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_entity_ts ON checkpoint_metadata(entity_type, entity_id, timestamp)'
    );

    logger.debug('Checkpoint schema created');
  }

  /**
   * Save checkpoint with metadata-BLOB separation and compression
   */
  async save(
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
        
        // Commit transaction
        await client.query('COMMIT');

        const elapsed = Date.now() - startTime;
        this.updateMetric('save', elapsed, data.length);

        logger.debug('Checkpoint saved', {
          checkpointId,
          dataSize: data.length,
        });
      } catch (error) {
        // Rollback on error
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      return this.handlePostgresError(error, 'save', { checkpointId });
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

    // Insert or update metadata
    await client.query(
      `INSERT INTO checkpoint_metadata (
        id, execution_id, workflow_id, entity_type, entity_id, timestamp, tags, custom_fields,
        blob_size, blob_hash, created_at, updated_at
      ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())
      ON CONFLICT (id) DO UPDATE SET
        execution_id = EXCLUDED.execution_id,
        workflow_id = EXCLUDED.workflow_id,
        entity_type = EXCLUDED.entity_type,
        entity_id = EXCLUDED.entity_id,
        timestamp = EXCLUDED.timestamp,
        tags = EXCLUDED.tags,
        custom_fields = EXCLUDED.custom_fields,
        blob_size = EXCLUDED.blob_size,
        blob_hash = EXCLUDED.blob_hash,
        updated_at = NOW()`,
      [
        checkpointId,
        metadata.executionId || null,
        metadata.workflowId || null,
        metadata.entityType || null,
        metadata.entityId || null,
        metadata.timestamp,
        metadata.tags ? JSON.stringify(metadata.tags) : null,
        metadata.customFields ? JSON.stringify(metadata.customFields) : null,
        compressed.length,
        blobHash,
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
  async load(checkpointId: string): Promise<Uint8Array | null> {
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
      return this.handlePostgresError(error, 'load', { checkpointId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete checkpoint
   */
  async delete(checkpointId: string): Promise<void> {
    const client = await this.getClient();

    try {
      await this.deleteFromClient(client, checkpointId);
      logger.debug('Checkpoint deleted', { checkpointId });
    } catch (error) {
      return this.handlePostgresError(error, 'delete', { checkpointId });
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
      const params: Array<string | number> = [];
      let paramIndex = 1;

      if (options?.executionId) {
        conditions.push(`execution_id = $${paramIndex++}`);
        params.push(options.executionId);
      }

      if (options?.workflowId) {
        conditions.push(`workflow_id = $${paramIndex++}`);
        params.push(options.workflowId);
      }

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
      return this.handlePostgresError(error, 'list', { options });
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
      return this.handlePostgresError(error, 'exists', { checkpointId });
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
        executionId: row.execution_id,
        workflowId: row.workflow_id,
        timestamp: row.timestamp,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.custom_fields,
      };

      return metadata;
    } catch (error) {
      return this.handlePostgresError(error, 'getMetadata', { checkpointId });
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
      const params: Array<string | number> = [];
      let paramIndex = 1;

      if (options?.executionId) {
        conditions.push(`execution_id = $${paramIndex++}`);
        params.push(options.executionId);
      }

      if (options?.workflowId) {
        conditions.push(`workflow_id = $${paramIndex++}`);
        params.push(options.workflowId);
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

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      const query = `
        SELECT 
          id,
          execution_id as "executionId",
          workflow_id as "workflowId",
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
          executionId: row.executionId || undefined,
          workflowId: row.workflowId || undefined,
          entityType: row.entityType || undefined,
          entityId: row.entityId || undefined,
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
      return this.handlePostgresError(error, 'listWithMetadata', { options });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete all checkpoints for an execution
   */
  async deleteByExecution(executionId: string): Promise<number> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        'DELETE FROM checkpoint_metadata WHERE execution_id = $1',
        [executionId]
      );
      
      logger.info('Checkpoints deleted by execution', {
        executionId,
        count: result.rowCount,
      });

      return result.rowCount || 0;
    } catch (error) {
      return this.handlePostgresError(error, 'deleteByExecution', { executionId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get latest checkpoint for an execution
   */
  async getLatestCheckpoint(executionId: string): Promise<string | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT id FROM checkpoint_metadata 
         WHERE execution_id = $1 
         ORDER BY timestamp DESC 
         LIMIT 1`,
        [executionId]
      );

      return result.rows.length > 0 ? result.rows[0].id : null;
    } catch (error) {
      return this.handlePostgresError(error, 'getLatestCheckpoint', { executionId });
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
      logger.info('All checkpoints cleared');
    } catch (error) {
      return this.handlePostgresError(error, 'clear', {});
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
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      return this.handlePostgresError(error, 'saveBatch', { count: items.length });
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
      return this.handlePostgresError(error, 'loadBatch', { count: ids.length });
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
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      return this.handlePostgresError(error, 'deleteBatch', { count: ids.length });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * List checkpoints for a specific entity
   */
  async listByEntity(
    entityId: string,
    entityType?: string,
    options?: Omit<CheckpointStorageListOptions, 'executionId' | 'workflowId'>
  ): Promise<string[]> {
    const client = await this.getClient();

    try {
      let sql = `SELECT id FROM checkpoint_metadata WHERE entity_id = $1`;
      const params: any[] = [entityId];

      if (entityType) {
        sql += ` AND entity_type = $${params.length + 1}`;
        params.push(entityType);
      }

      // Add additional filters from options
      if (options?.tags && options.tags.length > 0) {
        sql += ` AND tags LIKE $${params.length + 1}`;
        params.push(`%${options.tags[0]}%`);
      }

      if (options?.type) {
        sql += ` AND checkpoint_type = $${params.length + 1}`;
        params.push(options.type);
      }

      // Sort by timestamp descending
      sql += " ORDER BY timestamp DESC";

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
      return result.rows.map((row: any) => row.id);
    } catch (error) {
      return this.handlePostgresError(error, 'listByEntity', { entityId, entityType });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get the latest checkpoint for a specific entity
   */
  async getLatestByEntity(entityId: string, entityType?: string): Promise<string | null> {
    const client = await this.getClient();

    try {
      let sql = `SELECT id FROM checkpoint_metadata WHERE entity_id = $1`;
      const params: any[] = [entityId];

      if (entityType) {
        sql += ` AND entity_type = $${params.length + 1}`;
        params.push(entityType);
      }

      sql += " ORDER BY timestamp DESC LIMIT 1";

      const result = await client.query(sql, params);
      return result.rows.length > 0 ? result.rows[0].id : null;
    } catch (error) {
      return this.handlePostgresError(error, 'getLatestByEntity', { entityId, entityType });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete all checkpoints for a specific entity
   */
  async deleteByEntity(entityId: string, entityType?: string): Promise<number> {
    const client = await this.getClient();

    try {
      let sql = `DELETE FROM checkpoint_metadata WHERE entity_id = $1`;
      const params: any[] = [entityId];

      if (entityType) {
        sql += ` AND entity_type = $${params.length + 1}`;
        params.push(entityType);
      }

      const result = await client.query(sql, params);
      return parseInt(result.rowCount?.toString() || '0');
    } catch (error) {
      return this.handlePostgresError(error, 'deleteByEntity', { entityId, entityType });
    } finally {
      this.releaseClient(client);
    }
  }
}
