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
        execution_id TEXT NOT NULL,
        workflow_id TEXT NOT NULL,
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
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_timestamp ON checkpoint_metadata(timestamp)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_type ON checkpoint_metadata(checkpoint_type)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_cp_meta_exec_ts ON checkpoint_metadata(execution_id, timestamp)'
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
        id, execution_id, workflow_id, timestamp, tags, custom_fields,
        blob_size, blob_hash, created_at, updated_at
      ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
      ON CONFLICT (id) DO UPDATE SET
        execution_id = EXCLUDED.execution_id,
        workflow_id = EXCLUDED.workflow_id,
        timestamp = EXCLUDED.timestamp,
        tags = EXCLUDED.tags,
        custom_fields = EXCLUDED.custom_fields,
        blob_size = EXCLUDED.blob_size,
        blob_hash = EXCLUDED.blob_hash,
        updated_at = NOW()`,
      [
        checkpointId,
        metadata.executionId,
        metadata.workflowId,
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
}
