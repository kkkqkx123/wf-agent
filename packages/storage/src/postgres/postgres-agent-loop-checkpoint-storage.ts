/**
 * PostgreSQL Agent Loop Checkpoint Storage Implementation with Metadata-BLOB Separation
 * Agent loop checkpoint persistence storage based on pg
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type { 
  AgentCheckpointMetadata, 
  AgentCheckpointListOptions,
} from "@wf-agent/types";
import type { AgentLoopCheckpointStorageAdapter } from "../types/adapter/agent-loop-checkpoint-adapter.js";
import { BasePostgresStorage, BasePostgresStorageConfig } from "./base-postgres-storage.js";
import { selectCompressionStrategy } from "@wf-agent/common-utils";
import { compressBlob, decompressBlob } from "@wf-agent/common-utils";
import { PoolClient } from 'pg';
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("postgres-agent-loop-checkpoint-storage");

export class PostgresAgentLoopCheckpointStorage
  extends BasePostgresStorage<AgentCheckpointMetadata>
  implements AgentLoopCheckpointStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
    super(config);
  }

  /**
   * Get metadata table name
   */
  protected getTableName(): string {
    return "agent_loop_checkpoint_metadata";
  }

  /**
   * Get BLOB table name
   */
  protected getBlobTableName(): string | null {
    return "agent_loop_checkpoint_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected async createTableSchema(client: PoolClient): Promise<void> {
    // Layer 1: Metadata table (frequent queries, no BLOB)
    await client.query(`
      CREATE TABLE IF NOT EXISTS agent_loop_checkpoint_metadata (
        id TEXT PRIMARY KEY,
        agent_loop_id TEXT NOT NULL,
        timestamp BIGINT NOT NULL,
        type TEXT NOT NULL,
        version INTEGER,
        blob_size INTEGER,
        blob_hash TEXT,
        tags JSONB,
        custom_fields JSONB,
        created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
      )
    `);

    // Layer 2: BLOB storage table (infrequent direct access)
    await client.query(`
      CREATE TABLE IF NOT EXISTS agent_loop_checkpoint_blob (
        checkpoint_id TEXT PRIMARY KEY REFERENCES agent_loop_checkpoint_metadata(id) ON DELETE CASCADE,
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
      'CREATE INDEX IF NOT EXISTS idx_alcp_meta_agent_loop_id ON agent_loop_checkpoint_metadata(agent_loop_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_alcp_meta_timestamp ON agent_loop_checkpoint_metadata(timestamp)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_alcp_meta_type ON agent_loop_checkpoint_metadata(type)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_alcp_meta_agent_ts ON agent_loop_checkpoint_metadata(agent_loop_id, timestamp)'
    );

    logger.debug('Agent loop checkpoint schema created');
  }

  /**
   * Save agent loop checkpoint with metadata-BLOB separation and compression
   */
  async save(
    checkpointId: string,
    data: Uint8Array,
    metadata: AgentCheckpointMetadata
  ): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      await client.query('BEGIN');

      try {
        await this.saveToClient(client, checkpointId, data, metadata);
        await client.query('COMMIT');

        const elapsed = Date.now() - startTime;
        this.updateMetric('save', elapsed, data.length);

        logger.debug('Agent loop checkpoint saved', {
          checkpointId,
          dataSize: data.length,
        });
      } catch (error) {
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
    metadata: AgentCheckpointMetadata
  ): Promise<void> {
    // Get compression config
    const config = selectCompressionStrategy(data);

    // Compress BLOB data
    const { compressed, algorithm } = await compressBlob(data, config);
    
    // Compute blob hash
    const blobHash = await this.computeHash(data);

    // Insert or update metadata
    await client.query(
      `INSERT INTO agent_loop_checkpoint_metadata (
        id, agent_loop_id, timestamp, type, version, blob_size, blob_hash, tags, custom_fields
      ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
      ON CONFLICT (id) DO UPDATE SET
        agent_loop_id = EXCLUDED.agent_loop_id,
        timestamp = EXCLUDED.timestamp,
        type = EXCLUDED.type,
        version = EXCLUDED.version,
        blob_size = EXCLUDED.blob_size,
        blob_hash = EXCLUDED.blob_hash,
        tags = EXCLUDED.tags,
        custom_fields = EXCLUDED.custom_fields`,
      [
        checkpointId,
        metadata.agentLoopId,
        metadata.timestamp,
        metadata.type,
        metadata.version ?? null,
        compressed.length,
        blobHash,
        metadata.tags ? JSON.stringify(metadata.tags) : null,
        metadata.customFields ? JSON.stringify(metadata.customFields) : null,
      ]
    );

    // Insert or update BLOB data
    await client.query(
      `INSERT INTO agent_loop_checkpoint_blob (checkpoint_id, blob_data, compressed, compression_algorithm)
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
   * Load agent loop checkpoint with automatic decompression
   */
  async load(checkpointId: string): Promise<Uint8Array | null> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const result = await client.query(
        `SELECT alcb.blob_data, alcb.compressed, alcb.compression_algorithm
         FROM agent_loop_checkpoint_blob alcb
         WHERE alcb.checkpoint_id = $1`,
        [checkpointId]
      );

      if (result.rows.length === 0) {
        logger.debug('Agent loop checkpoint not found', { checkpointId });
        return null;
      }

      const row = result.rows[0];
      let data = row.blob_data;

      // Decompress if needed
      if (row.compressed && row.compression_algorithm) {
        data = await decompressBlob(data, row.compression_algorithm);
      }

      // Verify integrity if enabled
      if (this.shouldVerifyIntegrity()) {
        const metaResult = await client.query(
          'SELECT blob_hash FROM agent_loop_checkpoint_metadata WHERE id = $1',
          [checkpointId]
        );
        if (metaResult.rows.length > 0) {
          await this.verifyIntegrity(data, metaResult.rows[0].blob_hash, checkpointId);
        }
      }

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed, data.length);

      logger.debug('Agent loop checkpoint loaded', {
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
   * Delete agent loop checkpoint (cascade delete will handle blobs)
   */
  async delete(checkpointId: string): Promise<void> {
    const client = await this.getClient();

    try {
      await this.deleteFromClient(client, checkpointId);
      logger.debug('Agent loop checkpoint deleted', { checkpointId });
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
    await client.query(
      'DELETE FROM agent_loop_checkpoint_metadata WHERE id = $1',
      [checkpointId]
    );
  }

  /**
   * List agent loop checkpoint IDs (optimized - only scans metadata table)
   */
  async list(options?: AgentCheckpointListOptions): Promise<string[]> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Build dynamic query based on filters
      const conditions: string[] = [];
      const params: Array<string | number | string[]> = [];
      let paramIndex = 1;

      if (options?.agentLoopId) {
        conditions.push(`agent_loop_id = $${paramIndex++}`);
        params.push(options.agentLoopId);
      }

      if (options?.type) {
        conditions.push(`type = $${paramIndex++}`);
        params.push(options.type);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags ?| $${paramIndex++}`);
        params.push(options.tags);
      }

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      const query = `
        SELECT id FROM agent_loop_checkpoint_metadata
        ${whereClause}
        ORDER BY timestamp DESC
        LIMIT $${paramIndex++} OFFSET $${paramIndex++}
      `;

      params.push(limit, offset);

      const result = await client.query(query, params);
      const ids = result.rows.map(row => row.id);

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      logger.debug('Agent loop checkpoints listed', {
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
   * Check if agent loop checkpoint exists
   */
  async exists(checkpointId: string): Promise<boolean> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        'SELECT 1 FROM agent_loop_checkpoint_metadata WHERE id = $1',
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
   * Get agent loop checkpoint metadata
   */
  async getMetadata(checkpointId: string): Promise<AgentCheckpointMetadata | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT * FROM agent_loop_checkpoint_metadata WHERE id = $1`,
        [checkpointId]
      );

      if (result.rows.length === 0) {
        return null;
      }

      const row = result.rows[0];
      
      const metadata: AgentCheckpointMetadata = {
        agentLoopId: row.agent_loop_id,
        timestamp: row.timestamp,
        type: row.type,
        version: row.version ?? undefined,
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
   * List checkpoints for a specific agent loop
   */
  async listByAgentLoop(
    agentLoopId: string,
    options?: Omit<AgentCheckpointListOptions, 'agentLoopId'>
  ): Promise<string[]> {
    const client = await this.getClient();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Build dynamic query based on filters
      const conditions: string[] = ['agent_loop_id = $1'];
      const params: Array<string | number | string[]> = [agentLoopId];
      let paramIndex = 2;

      if (options?.type) {
        conditions.push(`type = $${paramIndex++}`);
        params.push(options.type);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags ?| $${paramIndex++}`);
        params.push(options.tags);
      }

      const whereClause = `WHERE ${conditions.join(' AND ')}`;

      const query = `
        SELECT id FROM agent_loop_checkpoint_metadata
        ${whereClause}
        ORDER BY timestamp DESC
        LIMIT $${paramIndex++} OFFSET $${paramIndex++}
      `;

      params.push(limit, offset);

      const result = await client.query(query, params);
      const ids = result.rows.map(row => row.id);

      logger.debug('Agent loop checkpoints listed by agent loop', {
        agentLoopId,
        count: ids.length,
      });

      return ids;
    } catch (error) {
      return this.handlePostgresError(error, 'listByAgentLoop', { agentLoopId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get the latest checkpoint ID for an agent loop
   */
  async getLatestCheckpoint(agentLoopId: string): Promise<string | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT id FROM agent_loop_checkpoint_metadata 
         WHERE agent_loop_id = $1 
         ORDER BY timestamp DESC 
         LIMIT 1`,
        [agentLoopId]
      );

      return result.rows.length > 0 ? result.rows[0].id : null;
    } catch (error) {
      return this.handlePostgresError(error, 'getLatestCheckpoint', { agentLoopId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete all checkpoints for an agent loop
   */
  async deleteByAgentLoop(agentLoopId: string): Promise<number> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        'DELETE FROM agent_loop_checkpoint_metadata WHERE agent_loop_id = $1',
        [agentLoopId]
      );

      const count = result.rowCount || 0;

      logger.info('Agent loop checkpoints deleted', {
        agentLoopId,
        count,
      });

      return count;
    } catch (error) {
      return this.handlePostgresError(error, 'deleteByAgentLoop', { agentLoopId });
    } finally {
      this.releaseClient(client);
    }
  }
}
