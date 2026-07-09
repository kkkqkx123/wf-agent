/**
 * PostgreSQL Task Storage Implementation with Metadata-BLOB Separation
 * Task persistence storage based on pg
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type { 
  TaskStorageMetadata, 
  TaskListOptions,
  TaskStats,
  TaskStatsOptions,
  TaskStatus,
} from "@wf-agent/types";
import type { TaskStorageAdapter } from "../types/adapter/index.js";
import { BasePostgresStorage, BasePostgresStorageConfig } from "./base-postgres-storage.js";
import { selectCompressionStrategy } from "@wf-agent/common-utils";
import { compressBlob, decompressBlob } from "@wf-agent/common-utils";
import { PoolClient } from 'pg';
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("postgres-task-storage");

export class PostgresTaskStorage
  extends BasePostgresStorage<TaskStorageMetadata, TaskListOptions>
  implements TaskStorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
    super(config);
  }

  /**
   * Get metadata table name
   */
  protected getTableName(): string {
    return "task_metadata";
  }

  /**
   * Get BLOB table name
   */
  protected getBlobTableName(): string | null {
    return "task_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected async createTableSchema(client: PoolClient): Promise<void> {
    // Layer 1: Metadata table (frequent queries, no BLOB)
    await client.query(`
      CREATE TABLE IF NOT EXISTS task_metadata (
        id TEXT PRIMARY KEY,
        execution_id TEXT NOT NULL,
        workflow_id TEXT NOT NULL,
        status TEXT NOT NULL,
        submit_time BIGINT NOT NULL,
        start_time BIGINT,
        complete_time BIGINT,
        timeout INTEGER,
        error TEXT,
        error_stack TEXT,
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
      CREATE TABLE IF NOT EXISTS task_blob (
        task_id TEXT PRIMARY KEY REFERENCES task_metadata(id) ON DELETE CASCADE,
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
      'CREATE INDEX IF NOT EXISTS idx_task_meta_execution_id ON task_metadata(execution_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_task_meta_workflow_id ON task_metadata(workflow_id)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_task_meta_status ON task_metadata(status)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_task_meta_submit_time ON task_metadata(submit_time)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_task_meta_complete_time ON task_metadata(complete_time)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_task_meta_status_submit ON task_metadata(status, submit_time)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_task_meta_created_at ON task_metadata(created_at)'
    );
    await client.query(
      'CREATE INDEX IF NOT EXISTS idx_task_meta_updated_at ON task_metadata(updated_at)'
    );

    logger.debug('Task schema created');
  }

  /**
   * Save task with metadata-BLOB separation and compression
   */
  async save(taskId: string, data: Uint8Array, metadata: TaskStorageMetadata): Promise<void> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      await client.query('BEGIN');

      try {
        await this.saveToClient(client, taskId, data, metadata);
        await client.query('COMMIT');

        const elapsed = Date.now() - startTime;
        this.updateMetric('save', elapsed, data.length);

        logger.debug('Task saved', {
          taskId,
          dataSize: data.length,
        });
      } catch (error) {
        await client.query('ROLLBACK');
        throw error;
      }
    } catch (error) {
      return this.handlePostgresError(error, 'save', { taskId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Save to client with compression
   */
  protected async saveToClient(
    client: PoolClient,
    taskId: string,
    data: Uint8Array,
    metadata: TaskStorageMetadata
  ): Promise<void> {
    // Get compression config
    const config = selectCompressionStrategy(data);

    // Compress BLOB data
    const { compressed, algorithm } = await compressBlob(data, config);
    
    // Compute blob hash
    const blobHash = await this.computeHash(data);

    // Insert or update metadata
    await client.query(
      `INSERT INTO task_metadata (
        id, execution_id, workflow_id, status, submit_time, start_time,
        complete_time, timeout, error, error_stack,
        blob_size, blob_hash, tags, custom_fields, created_at, updated_at
      ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NOW(), NOW())
      ON CONFLICT (id) DO UPDATE SET
        execution_id = EXCLUDED.execution_id,
        workflow_id = EXCLUDED.workflow_id,
        status = EXCLUDED.status,
        submit_time = EXCLUDED.submit_time,
        start_time = EXCLUDED.start_time,
        complete_time = EXCLUDED.complete_time,
        timeout = EXCLUDED.timeout,
        error = EXCLUDED.error,
        error_stack = EXCLUDED.error_stack,
        blob_size = EXCLUDED.blob_size,
        blob_hash = EXCLUDED.blob_hash,
        tags = EXCLUDED.tags,
        custom_fields = EXCLUDED.custom_fields,
        updated_at = NOW()`,
      [
        taskId,
        metadata.executionId,
        metadata.workflowId,
        metadata.status,
        metadata.submitTime,
        metadata.startTime ?? null,
        metadata.completeTime ?? null,
        metadata.timeout ?? null,
        metadata.error ?? null,
        metadata.errorStack ?? null,
        compressed.length,
        blobHash,
        metadata.tags ? JSON.stringify(metadata.tags) : null,
        metadata.customFields ? JSON.stringify(metadata.customFields) : null,
      ]
    );

    // Insert or update BLOB data
    await client.query(
      `INSERT INTO task_blob (task_id, blob_data, compressed, compression_algorithm)
       VALUES ($1, $2, $3, $4)
       ON CONFLICT (task_id) DO UPDATE SET
         blob_data = EXCLUDED.blob_data,
         compressed = EXCLUDED.compressed,
         compression_algorithm = EXCLUDED.compression_algorithm`,
      [
        taskId,
        Buffer.from(compressed),
        algorithm !== null,
        algorithm,
      ]
    );
  }

  /**
   * Load task data with automatic decompression
   */
  async load(taskId: string): Promise<Uint8Array | null> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const result = await client.query(
        `SELECT tb.blob_data, tb.compressed, tb.compression_algorithm
         FROM task_blob tb
         WHERE tb.task_id = $1`,
        [taskId]
      );

      if (result.rows.length === 0) {
        logger.debug('Task not found', { taskId });
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
          'SELECT blob_hash FROM task_metadata WHERE id = $1',
          [taskId]
        );
        if (metaResult.rows.length > 0) {
          await this.verifyIntegrity(data, metaResult.rows[0].blob_hash, taskId);
        }
      }

      const elapsed = Date.now() - startTime;
      this.updateMetric('load', elapsed, data.length);

      logger.debug('Task loaded', {
        taskId,
        dataSize: data.length,
        compressed: row.compressed,
      });

      return data;
    } catch (error) {
      return this.handlePostgresError(error, 'load', { taskId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete task (cascade delete will handle blobs)
   */
  async delete(taskId: string): Promise<void> {
    const client = await this.getClient();

    try {
      await this.deleteFromClient(client, taskId);
      logger.debug('Task deleted', { taskId });
    } catch (error) {
      return this.handlePostgresError(error, 'delete', { taskId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Delete from client
   */
  protected async deleteFromClient(client: PoolClient, taskId: string): Promise<void> {
    await client.query(
      'DELETE FROM task_metadata WHERE id = $1',
      [taskId]
    );
  }

  /**
   * List task IDs (optimized - only scans metadata table)
   */
  async list(options?: TaskListOptions): Promise<string[]> {
    const client = await this.getClient();
    const startTime = Date.now();

    try {
      const { limit, offset } = this.validatePagination(options?.limit, options?.offset);

      // Build dynamic query based on filters
      const conditions: string[] = [];
      const params: Array<string | number | string[]> = [];
      let paramIndex = 1;

      if (options?.executionId) {
        conditions.push(`execution_id = $${paramIndex++}`);
        params.push(options.executionId);
      }

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

      if (options?.submitTimeFrom) {
        conditions.push(`submit_time >= $${paramIndex++}`);
        params.push(options.submitTimeFrom);
      }

      if (options?.submitTimeTo) {
        conditions.push(`submit_time <= $${paramIndex++}`);
        params.push(options.submitTimeTo);
      }

      if (options?.startTimeFrom) {
        conditions.push(`start_time >= $${paramIndex++}`);
        params.push(options.startTimeFrom);
      }

      if (options?.startTimeTo) {
        conditions.push(`start_time <= $${paramIndex++}`);
        params.push(options.startTimeTo);
      }

      if (options?.completeTimeFrom) {
        conditions.push(`complete_time >= $${paramIndex++}`);
        params.push(options.completeTimeFrom);
      }

      if (options?.completeTimeTo) {
        conditions.push(`complete_time <= $${paramIndex++}`);
        params.push(options.completeTimeTo);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags ?| $${paramIndex++}`);
        params.push(options.tags);
      }

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      // Sort by validated column
      const sortBy = options?.sortBy ?? 'submitTime';
      const sortOrder = options?.sortOrder ?? 'desc';
      
      const sortColumnMap: Record<string, string> = {
        submitTime: 'submit_time',
        startTime: 'start_time',
        completeTime: 'complete_time',
      };
      
      const sortColumn = sortColumnMap[sortBy] || 'submit_time';
      const orderDirection = sortOrder.toLowerCase() === 'asc' ? 'ASC' : 'DESC';

      const query = `
        SELECT id FROM task_metadata
        ${whereClause}
        ORDER BY ${sortColumn} ${orderDirection}
        LIMIT $${paramIndex++} OFFSET $${paramIndex++}
      `;

      params.push(limit, offset);

      const result = await client.query(query, params);
      const ids = result.rows.map(row => row.id);

      const elapsed = Date.now() - startTime;
      this.updateMetric('list', elapsed);

      logger.debug('Tasks listed', {
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
   * Check if task exists
   */
  async exists(taskId: string): Promise<boolean> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        'SELECT 1 FROM task_metadata WHERE id = $1',
        [taskId]
      );
      return result.rows.length > 0;
    } catch (error) {
      return this.handlePostgresError(error, 'exists', { taskId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get task metadata
   */
  async getMetadata(taskId: string): Promise<TaskStorageMetadata | null> {
    const client = await this.getClient();

    try {
      const result = await client.query(
        `SELECT * FROM task_metadata WHERE id = $1`,
        [taskId]
      );

      if (result.rows.length === 0) {
        return null;
      }

      const row = result.rows[0];
      
      const metadata: TaskStorageMetadata = {
        taskId: row.id,
        executionId: row.execution_id,
        workflowId: row.workflow_id,
        status: row.status,
        submitTime: row.submit_time,
        startTime: row.start_time ?? undefined,
        completeTime: row.complete_time ?? undefined,
        timeout: row.timeout ?? undefined,
        error: row.error ?? undefined,
        errorStack: row.error_stack ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.custom_fields,
      };

      return metadata;
    } catch (error) {
      return this.handlePostgresError(error, 'getMetadata', { taskId });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Get task statistics
   */
  async getTaskStats(options?: TaskStatsOptions): Promise<TaskStats> {
    const client = await this.getClient();

    try {
      const conditions: string[] = [];
      const params: Array<string | number> = [];
      let paramIndex = 1;

      if (options?.workflowId) {
        conditions.push(`workflow_id = $${paramIndex++}`);
        params.push(options.workflowId);
      }

      if (options?.timeFrom) {
        conditions.push(`submit_time >= $${paramIndex++}`);
        params.push(options.timeFrom);
      }

      if (options?.timeTo) {
        conditions.push(`submit_time <= $${paramIndex++}`);
        params.push(options.timeTo);
      }

      const whereClause = conditions.length > 0 
        ? `WHERE ${conditions.join(' AND ')}`
        : '';

      // Get total count
      const totalResult = await client.query(
        `SELECT COUNT(*) as count FROM task_metadata ${whereClause}`,
        params
      );
      const total = parseInt(totalResult.rows[0].count, 10);

      // Get counts by status
      const statusResult = await client.query(
        `SELECT status, COUNT(*) as count 
         FROM task_metadata 
         ${whereClause}
         GROUP BY status`,
        params
      );

      const byStatus: Record<TaskStatus, number> = {
        QUEUED: 0,
        RUNNING: 0,
        COMPLETED: 0,
        FAILED: 0,
        CANCELLED: 0,
        TIMEOUT: 0,
      };

      statusResult.rows.forEach((row) => {
        const status = row.status as TaskStatus;
        if (status in byStatus) {
          byStatus[status] = parseInt(row.count, 10);
        }
      });

      // Get counts by workflow
      const workflowResult = await client.query(
        `SELECT workflow_id, COUNT(*) as count 
         FROM task_metadata 
         ${whereClause}
         GROUP BY workflow_id`,
        params
      );

      const byWorkflow: Record<string, number> = {};
      workflowResult.rows.forEach((row) => {
        byWorkflow[row.workflow_id] = parseInt(row.count, 10);
      });

      // Calculate execution time statistics from start_time/complete_time
      const execTimeResult = await client.query(
        `SELECT 
           AVG(complete_time - start_time) as avg_time,
           MAX(complete_time - start_time) as max_time,
           MIN(complete_time - start_time) as min_time
         FROM task_metadata 
         ${whereClause ? whereClause + ' AND' : 'WHERE'}
         start_time IS NOT NULL AND complete_time IS NOT NULL`,
        params
      );

      const row = execTimeResult.rows[0];
      const avgExecutionTime = row.avg_time ? parseFloat(row.avg_time) : undefined;
      const maxExecutionTime = row.max_time ? parseInt(row.max_time, 10) : undefined;
      const minExecutionTime = row.min_time ? parseInt(row.min_time, 10) : undefined;

      // Calculate success rate
      const completedCount = byStatus.COMPLETED || 0;
      const successRate = total > 0 ? completedCount / total : undefined;

      // Calculate timeout rate
      const timeoutCount = byStatus.TIMEOUT || 0;
      const timeoutRate = total > 0 ? timeoutCount / total : undefined;

      const stats: TaskStats = {
        total,
        byStatus,
        byWorkflow,
        avgExecutionTime,
        maxExecutionTime,
        minExecutionTime,
        successRate,
        timeoutRate,
      };

      logger.debug('Task stats retrieved', { total });

      return stats;
    } catch (error) {
      return this.handlePostgresError(error, 'getTaskStats', { options });
    } finally {
      this.releaseClient(client);
    }
  }

  /**
   * Clean up expired tasks
   */
  async cleanupTasks(retentionTime: number): Promise<number> {
    const client = await this.getClient();
    const cutoffTime = Date.now() - retentionTime;

    try {
      const result = await client.query(
        `DELETE FROM task_metadata 
         WHERE status IN ('COMPLETED', 'FAILED', 'CANCELLED', 'TIMEOUT')
         AND complete_time IS NOT NULL 
         AND complete_time < $1`,
        [cutoffTime]
      );

      const count = result.rowCount || 0;

      logger.info('Tasks cleaned up', { count, retentionTime });

      return count;
    } catch (error) {
      return this.handlePostgresError(error, 'cleanupTasks', { retentionTime });
    } finally {
      this.releaseClient(client);
    }
  }
}
