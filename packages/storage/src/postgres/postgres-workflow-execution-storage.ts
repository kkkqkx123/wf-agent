/**
 * PostgreSQL Workflow Execution Storage Implementation with Metadata-BLOB Separation
 * Workflow execution persistent storage based on pg
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type {
  WorkflowExecutionStorageMetadata,
  WorkflowExecutionListOptions,
  WorkflowExecutionStatus,
} from "@wf-agent/types";
import type { WorkflowExecutionStorageAdapter } from "../types/adapter/workflow-execution-adapter.js";
import { BasePostgresStorage, BasePostgresStorageConfig } from "./base-postgres-storage.js";
import { selectCompressionStrategy } from "@wf-agent/common-utils";
import { compressBlob, decompressBlob } from "@wf-agent/common-utils";
import { PoolClient } from 'pg';
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("postgres-workflow-execution-storage");

export class PostgresWorkflowExecutionStorage
  extends BasePostgresStorage<WorkflowExecutionStorageMetadata>
  implements WorkflowExecutionStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
    super(config);
  }

  /**
   * Get metadata table name
   */
  protected getTableName(): string {
    return "workflow_execution_metadata";
  }

  /**
   * Get BLOB table name
   */
  protected getBlobTableName(): string | null {
    return "workflow_execution_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected async createTableSchema(client: PoolClient): Promise<void> {
    // Layer 1: Metadata table (frequent queries, no BLOB)
    await client.query(`
      CREATE TABLE IF NOT EXISTS workflow_execution_metadata (
        id TEXT PRIMARY KEY,
        workflow_id TEXT NOT NULL,
        workflow_version TEXT NOT NULL,
        status TEXT NOT NULL,
        execution_type TEXT,
        current_node_id TEXT,
        parent_execution_id TEXT,
        start_time BIGINT NOT NULL,
        end_time BIGINT,
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
      CREATE TABLE IF NOT EXISTS workflow_execution_blob (
        execution_id TEXT PRIMARY KEY REFERENCES workflow_execution_metadata(id) ON DELETE CASCADE,
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
      'CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_workflow_id ON workflow_execution_metadata(workflow_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_status ON workflow_execution_metadata(status)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_start_time ON workflow_execution_metadata(start_time)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_end_time ON workflow_execution_metadata(end_time)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_parent ON workflow_execution_metadata(parent_execution_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_workflow_status ON workflow_execution_metadata(workflow_id, status)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_workflow_start ON workflow_execution_metadata(workflow_id, start_time)'
    );

    logger.debug('Workflow execution schema created');
  }

  /**
   * Save workflow execution with metadata-BLOB separation and compression
   */
  async save(
    executionId: string,
    data: Uint8Array,
    metadata: WorkflowExecutionStorageMetadata
  ): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      await client.query('BEGIN');

      try {
        await this.saveToClient(client, executionId, data, metadata);
        await client.query('COMMIT');

        const elapsed = Date.now() - startTime;
        this.updateMetric('save', elapsed, data.length);

        logger.debug('Workflow execution saved', {
          executionId,
          dataSize: data.length,
        });
      } catch (error) {
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      return this.handlePostgresError(error, 'save', { executionId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save to client with compression
   */
  protected async saveToClient(
    client: PoolClient,
    executionId: string,
    data: Uint8Array,
    metadata: WorkflowExecutionStorageMetadata
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
      `INSERT INTO workflow_execution_metadata (
        id, workflow_id, workflow_version, status, execution_type,
        current_node_id, parent_execution_id, start_time, end_time,
        blob_size, blob_hash, tags, custom_fields, created_at, updated_at
      ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), NOW())
      ON CONFLICT (id) DO UPDATE SET
        workflow_id = EXCLUDED.workflow_id,
        workflow_version = EXCLUDED.workflow_version,
        status = EXCLUDED.status,
        execution_type = EXCLUDED.execution_type,
        current_node_id = EXCLUDED.current_node_id,
        parent_execution_id = EXCLUDED.parent_execution_id,
        start_time = EXCLUDED.start_time,
        end_time = EXCLUDED.end_time,
        blob_size = EXCLUDED.blob_size,
        blob_hash = EXCLUDED.blob_hash,
        tags = EXCLUDED.tags,
        custom_fields = EXCLUDED.custom_fields,
        updated_at = NOW()`,
      [
        executionId,
        metadata.workflowId,
        metadata.workflowVersion,
        metadata.status,
        metadata.executionType ?? null,
        metadata.currentNodeId ?? null,
        metadata.parentExecutionId ?? null,
        metadata.startTime,
        metadata.endTime ?? null,
        compressed.length,
        blobHash,
        metadata.tags ? JSON.stringify(metadata.tags) : null,
        metadata.customFields ? JSON.stringify(metadata.customFields) : null,
      ]
    );

    // Insert or update BLOB data
    await client.query(
      `INSERT INTO workflow_execution_blob (execution_id, blob_data, compressed, compression_algorithm)
       VALUES ($1, $2, $3, $4)
       ON CONFLICT (execution_id) DO UPDATE SET
         blob_data = EXCLUDED.blob_data,
         compressed = EXCLUDED.compressed,
         compression_algorithm = EXCLUDED.compression_algorithm`,
      [
        executionId,
        Buffer.from(compressed),
        algorithm !== null,
        algorithm,
      ]
    );
  }

  /**
   * Load workflow execution data with automatic decompression
   */
  async load(executionId: string): Promise<Uint8Array | null> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const result = await client.query(
        `SELECT web.blob_data, web.compressed, web.compression_algorithm
         FROM workflow_execution_blob web
         WHERE web.execution_id = $1`,
        [executionId]
      );

      if (result.rows.length === 0) {
        logger.debug('Workflow execution not found', { executionId });
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
          'SELECT blob_hash FROM workflow_execution_metadata WHERE id = $1',
          [executionId]
        );
        if (metaResult.rows.length > 0) {
          await this.verifyIntegrity(data, metaResult.rows[0].blob_hash, executionId);
        }
      }

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed, data.length);

      logger.debug('Workflow execution loaded', {
        executionId,
        dataSize: data.length,
        compressed: row.compressed,
      });

      return data;
    } catch (error) {
      return this.handlePostgresError(error, 'load', { executionId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete workflow execution (cascade delete will handle blobs)
   */
  async delete(executionId: string): Promise<void> {
    const client = await this.getClient();

    try {
      await this.deleteFromClient(client, executionId);
      logger.debug('Workflow execution deleted', { executionId });
    } catch (error) {
      return this.handlePostgresError(error, 'delete', { executionId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete from client
   */
  protected async deleteFromClient(client: PoolClient, executionId: string): Promise<void> {
    await client.query(
      'DELETE FROM workflow_execution_metadata WHERE id = $1',
      [executionId]
    );
  }

  /**
   * List workflow execution IDs (optimized - only scans metadata table)
   */
  async list(options?: WorkflowExecutionListOptions): Promise<string[]> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Build dynamic query based on filters
      const conditions: string[] = [];
      const params: any[] = [];
      let paramIndex = 1;

      if (options?.workflowId) {
        conditions.push(`workflow_id = $${paramIndex++}`);
        params.push(options.workflowId);
      }

      if (options?.status) {
        if (Array.isArray(options.status)) {
          conditions.push(`status = ANY($${paramIndex++})`);
          params.push(options.status);
        } else {
          conditions.push(`status = $${paramIndex++}`);
          params.push(options.status);
        }
      }

      if (options?.executionType) {
        conditions.push(`execution_type = $${paramIndex++}`);
        params.push(options.executionType);
      }

      if (options?.parentExecutionId) {
        conditions.push(`parent_execution_id = $${paramIndex++}`);
        params.push(options.parentExecutionId);
      }

      if (options?.startTimeFrom) {
        conditions.push(`start_time >= $${paramIndex++}`);
        params.push(options.startTimeFrom);
      }

      if (options?.startTimeTo) {
        conditions.push(`start_time <= $${paramIndex++}`);
        params.push(options.startTimeTo);
      }

      if (options?.endTimeFrom) {
        conditions.push(`end_time >= $${paramIndex++}`);
        params.push(options.endTimeFrom);
      }

      if (options?.endTimeTo) {
        conditions.push(`end_time <= $${paramIndex++}`);
        params.push(options.endTimeTo);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags ?| $${paramIndex++}`);
        params.push(options.tags);
      }

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      // Sort by validated column
      const sortBy = options?.sortBy ?? 'startTime';
      const sortOrder = options?.sortOrder ?? 'desc';
      
      const sortColumnMap: Record<string, string> = {
        startTime: 'start_time',
        endTime: 'end_time',
        updatedAt: 'updated_at',
      };
      
      const sortColumn = sortColumnMap[sortBy] || 'start_time';
      const orderDirection = sortOrder.toLowerCase() === 'asc' ? 'ASC' : 'DESC';

      const query = `
        SELECT id FROM workflow_execution_metadata
        ${whereClause}
        ORDER BY ${sortColumn} ${orderDirection}
        LIMIT $${paramIndex++} OFFSET $${paramIndex++}
      `;

      params.push(limit, offset);

      const result = await client.query(query, params);
      const ids = result.rows.map(row => row.id);

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      logger.debug('Workflow executions listed', {
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
   * Check if workflow execution exists
   */
  async exists(executionId: string): Promise<boolean> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        'SELECT 1 FROM workflow_execution_metadata WHERE id = $1',
        [executionId]
      );
      return result.rows.length > 0;
    } catch (error) {
      return this.handlePostgresError(error, 'exists', { executionId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get workflow execution metadata
   */
  async getMetadata(executionId: string): Promise<WorkflowExecutionStorageMetadata | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT * FROM workflow_execution_metadata WHERE id = $1`,
        [executionId]
      );

      if (result.rows.length === 0) {
        return null;
      }

      const row = result.rows[0];
      
      const metadata: WorkflowExecutionStorageMetadata = {
        executionId: row.id,
        workflowId: row.workflow_id,
        workflowVersion: row.workflow_version,
        status: row.status,
        executionType: row.execution_type ?? undefined,
        currentNodeId: row.current_node_id ?? undefined,
        parentExecutionId: row.parent_execution_id ?? undefined,
        startTime: row.start_time,
        endTime: row.end_time ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.custom_fields,
      };

      return metadata;
    } catch (error) {
      return this.handlePostgresError(error, 'getMetadata', { executionId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Update workflow execution status
   */
  async updateExecutionStatus(
    executionId: string,
    status: WorkflowExecutionStatus
  ): Promise<void> {
    const client = await this.getClient();

    try {
      const now = Date.now();
      
      // If status is completed or failed, set end_time
      const endTimeUpdate = 
        status === 'COMPLETED' || status === 'FAILED' || status === 'CANCELLED'
          ? ', end_time = $2'
          : '';
      
      const params = status === 'COMPLETED' || status === 'FAILED' || status === 'CANCELLED'
        ? [status, now, executionId]
        : [status, executionId];

      await client.query(
        `UPDATE workflow_execution_metadata 
         SET status = $1${endTimeUpdate}, updated_at = NOW()
         WHERE id = $${params.length}`,
        params
      );

      logger.debug('Workflow execution status updated', { executionId, status });
    } catch (error) {
      return this.handlePostgresError(error, 'updateExecutionStatus', { executionId, status });
    } finally {
      this.releaseClient(client);
    }
  }
}
