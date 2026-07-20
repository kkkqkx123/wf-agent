/**
 * PostgreSQL Agent Loop Storage Implementation with Metadata-BLOB Separation
 * Agent loop lifecycle persistence storage based on pg
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type { 
  AgentEntityMetadata, 
  AgentEntityListOptions,
} from "@wf-agent/types";
import type { AgentLoopStorageAdapter } from "../types/adapter/agent-loop-adapter.js";
import { BasePostgresStorage, BasePostgresStorageConfig } from "./base-postgres-storage.js";
import { selectCompressionStrategy } from "@wf-agent/common-utils";
import { compressBlob, decompressBlob } from "@wf-agent/common-utils";
import { PoolClient } from 'pg';
import { createModuleLogger } from "../logger.js";
import { AgentLoopStatus } from "@wf-agent/types";

const logger = createModuleLogger("postgres-agent-loop-storage");

export class PostgresAgentLoopStorage
  extends BasePostgresStorage<AgentEntityMetadata, AgentEntityListOptions>
  implements AgentLoopStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
    super(config);
  }

  /**
   * Get metadata table name
   */
  protected getTableName(): string {
    return "agent_loop_metadata";
  }

  /**
   * Get BLOB table name
   */
  protected getBlobTableName(): string | null {
    return "agent_loop_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected async createTableSchema(client: PoolClient): Promise<void> {
    // Layer 1: Metadata table (frequent queries, no BLOB)
    await client.query(`
      CREATE TABLE IF NOT EXISTS agent_loop_metadata (
        id TEXT PRIMARY KEY,
        profile_id TEXT,
        status TEXT NOT NULL,
        created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
        completed_at TIMESTAMP WITH TIME ZONE,
        blob_size INTEGER,
        blob_hash TEXT,
        tags JSONB,
        custom_fields JSONB
      )
    `);

    // Layer 2: BLOB storage table (infrequent direct access)
    await client.query(`
      CREATE TABLE IF NOT EXISTS agent_loop_blob (
        agent_loop_id TEXT PRIMARY KEY REFERENCES agent_loop_metadata(id) ON DELETE CASCADE,
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
      'CREATE INDEX IF NOT EXISTS idx_al_meta_status ON agent_loop_metadata(status)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_al_meta_profile_id ON agent_loop_metadata(profile_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_al_meta_created_at ON agent_loop_metadata(created_at)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_al_meta_status_created ON agent_loop_metadata(status, created_at)'
    );

    logger.debug('Agent loop schema created');
  }

  /**
   * Save agent loop with metadata-BLOB separation and compression
   */
  async doSave(
    agentLoopId: string,
    data: Uint8Array,
    metadata: AgentEntityMetadata
  ): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      await client.query('BEGIN');

      try {
        await this.saveToClient(client, agentLoopId, data, metadata);
        await client.query('COMMIT');

        const elapsed = Date.now() - startTime;
        this.updateMetric('save', elapsed, data.length);

        logger.debug('Agent loop saved', {
          agentLoopId,
          dataSize: data.length,
        });
      } catch (error) {
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      return this.handlePostgresError(error, 'save', { agentLoopId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save to client with compression
   */
  protected async saveToClient(
    client: PoolClient,
    agentLoopId: string,
    data: Uint8Array,
    metadata: AgentEntityMetadata
  ): Promise<void> {
    const now = Date.now();

    // Get compression config
    const config = selectCompressionStrategy(data);

    // Compress BLOB data
    const { compressed, algorithm } = await compressBlob(data, config);
    
    // Compute blob hash
    const blobHash = await this.computeHash(data);

    // Insert or update metadata
    await client.query(
      `INSERT INTO agent_loop_metadata (
        id, profile_id, status, created_at, updated_at,
        completed_at, blob_size, blob_hash, tags, custom_fields
      ) VALUES ($1, $2, $3, TO_TIMESTAMP($4), TO_TIMESTAMP($5),
                CASE WHEN $6 IS NOT NULL THEN TO_TIMESTAMP($6) ELSE NULL END,
                $7, $8, $9, $10)
      ON CONFLICT (id) DO UPDATE SET
        profile_id = EXCLUDED.profile_id,
        status = EXCLUDED.status,
        created_at = EXCLUDED.created_at,
        updated_at = NOW(),
        completed_at = EXCLUDED.completed_at,
        blob_size = EXCLUDED.blob_size,
        blob_hash = EXCLUDED.blob_hash,
        tags = EXCLUDED.tags,
        custom_fields = EXCLUDED.custom_fields`,
      [
        agentLoopId,
        metadata.profileId ?? metadata.agentLoopId,
        metadata.status,
        metadata.createdAt / 1000,
        now / 1000,
        metadata.completedAt ? metadata.completedAt / 1000 : null,
        compressed.length,
        blobHash,
        metadata.tags ? JSON.stringify(metadata.tags) : null,
        metadata.customFields ? JSON.stringify(metadata.customFields) : null,
      ]
    );

    // Insert or update BLOB data
    await client.query(
      `INSERT INTO agent_loop_blob (agent_loop_id, blob_data, compressed, compression_algorithm)
       VALUES ($1, $2, $3, $4)
       ON CONFLICT (agent_loop_id) DO UPDATE SET
         blob_data = EXCLUDED.blob_data,
         compressed = EXCLUDED.compressed,
         compression_algorithm = EXCLUDED.compression_algorithm`,
      [
        agentLoopId,
        Buffer.from(compressed),
        algorithm !== null,
        algorithm,
      ]
    );
  }

  /**
   * Load agent loop data with automatic decompression
   */
  async doLoad(agentLoopId: string): Promise<Uint8Array | null> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const result = await client.query(
        `SELECT alb.blob_data, alb.compressed, alb.compression_algorithm
         FROM agent_loop_blob alb
         WHERE alb.agent_loop_id = $1`,
        [agentLoopId]
      );

      if (result.rows.length === 0) {
        logger.debug('Agent loop not found', { agentLoopId });
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
          'SELECT blob_hash FROM agent_loop_metadata WHERE id = $1',
          [agentLoopId]
        );
        if (metaResult.rows.length > 0) {
          await this.verifyIntegrity(data, metaResult.rows[0].blob_hash, agentLoopId);
        }
      }

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed, data.length);

      logger.debug('Agent loop loaded', {
        agentLoopId,
        dataSize: data.length,
        compressed: row.compressed,
      });

      return data;
    } catch (error) {
      return this.handlePostgresError(error, 'load', { agentLoopId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete agent loop (cascade delete will handle blobs)
   */
  async doDelete(agentLoopId: string): Promise<void> {
    const client = await this.getClient();

    try {
      await this.deleteFromClient(client, agentLoopId);
      logger.debug('Agent loop deleted', { agentLoopId });
    } catch (error) {
      return this.handlePostgresError(error, 'delete', { agentLoopId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete from client
   */
  protected async deleteFromClient(client: PoolClient, agentLoopId: string): Promise<void> {
    await client.query(
      'DELETE FROM agent_loop_metadata WHERE id = $1',
      [agentLoopId]
    );
  }

  /**
   * List agent loop IDs (optimized - only scans metadata table)
   */
  async list(options?: AgentEntityListOptions): Promise<string[]> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Build dynamic query based on filters
      const conditions: string[] = [];
      const params: Array<string | number | string[]> = [];
      let paramIndex = 1;

      if (options?.status) {
        conditions.push(`status = $${paramIndex++}`);
        params.push(options.status);
      }

      if (options?.profileId) {
        conditions.push(`profile_id = $${paramIndex++}`);
        params.push(options.profileId);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags ?| $${paramIndex++}`);
        params.push(options.tags);
      }

      if (options?.createdAfter) {
        conditions.push(`created_at >= TO_TIMESTAMP($${paramIndex++})`);
        params.push(options.createdAfter / 1000);
      }

      if (options?.createdBefore) {
        conditions.push(`created_at <= TO_TIMESTAMP($${paramIndex++})`);
        params.push(options.createdBefore / 1000);
      }

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      const query = `
        SELECT id FROM agent_loop_metadata
        ${whereClause}
        ORDER BY created_at DESC
        LIMIT $${paramIndex++} OFFSET $${paramIndex++}
      `;

      params.push(limit, offset);

      const result = await client.query(query, params);
      const ids = result.rows.map(row => row.id);

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      logger.debug('Agent loops listed', {
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
   * Check if agent loop exists
   */
  async exists(agentLoopId: string): Promise<boolean> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        'SELECT 1 FROM agent_loop_metadata WHERE id = $1',
        [agentLoopId]
      );
      return result.rows.length > 0;
    } catch (error) {
      return this.handlePostgresError(error, 'exists', { agentLoopId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get agent loop metadata
   */
  async getMetadata(agentLoopId: string): Promise<AgentEntityMetadata | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT * FROM agent_loop_metadata WHERE id = $1`,
        [agentLoopId]
      );

      if (result.rows.length === 0) {
        return null;
      }

      const row = result.rows[0];
      
      const metadata: AgentEntityMetadata = {
        agentLoopId: row.id,
        status: row.status,
        createdAt: new Date(row.created_at).getTime(),
        updatedAt: new Date(row.updated_at).getTime(),
        completedAt: row.completed_at ? new Date(row.completed_at).getTime() : undefined,
        profileId: row.profile_id ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.custom_fields,
      };

      return metadata;
    } catch (error) {
      return this.handlePostgresError(error, 'getMetadata', { agentLoopId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Update agent loop status
   */
  async updateAgentLoopStatus(
    agentLoopId: string,
    status: AgentLoopStatus
  ): Promise<void> {
    const client = await this.getClient();

    try {
      const now = Date.now();
      
      // Set completed_at for terminal statuses
      const terminalStatuses: AgentLoopStatus[] = ["COMPLETED", "FAILED", "CANCELLED"];
      const isTerminal = terminalStatuses.includes(status);
      
      const completedUpdate = isTerminal
        ? ', completed_at = TO_TIMESTAMP($2)'
        : '';
      
      const params = isTerminal
        ? [status, now / 1000, agentLoopId]
        : [status, agentLoopId];

      await client.query(
        `UPDATE agent_loop_metadata 
         SET status = $1${completedUpdate}, updated_at = NOW()
         WHERE id = $${params.length}`,
        params
      );

      logger.debug('Agent loop status updated', { agentLoopId, status });
    } catch (error) {
      return this.handlePostgresError(error, 'updateAgentLoopStatus', { agentLoopId, status });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * List agent loops by status
   */
  async listByStatus(status: AgentLoopStatus): Promise<string[]> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT id FROM agent_loop_metadata WHERE status = $1 ORDER BY created_at DESC`,
        [status]
      );

      const ids = result.rows.map(row => row.id);

      logger.debug('Agent loops listed by status', { status, count: ids.length });

      return ids;
    } catch (error) {
      return this.handlePostgresError(error, 'listByStatus', { status });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get agent loop statistics
   */
  async getAgentLoopStats(): Promise<{ total: number; byStatus: Record<string, number> }> {
    const client = await this.getClient();

    try {
      // Get total count
      const totalResult = await client.query(
        'SELECT COUNT(*) as count FROM agent_loop_metadata'
      );
      const total = parseInt(totalResult.rows[0].count, 10);

      // Get counts by status
      const statusResult = await client.query(
        'SELECT status, COUNT(*) as count FROM agent_loop_metadata GROUP BY status'
      );

      const byStatus: Record<string, number> = {};
      statusResult.rows.forEach((row) => {
        byStatus[row.status] = parseInt(row.count, 10);
      });

      logger.debug('Agent loop stats retrieved', { total });

      return { total, byStatus };
    } catch (error) {
      return this.handlePostgresError(error, 'getAgentLoopStats', {});
    } finally {
      this.releaseClient(client);
    }
  }
}
