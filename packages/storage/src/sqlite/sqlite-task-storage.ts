/**
 * SQLite Task Storage Implementation with Metadata-BLOB Separation
 * Task persistence storage based on better-sqlite3
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
import { BaseSqliteStorage, BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
import { selectCompressionStrategy } from "../compression/adaptive-compression.js";
import { compressBlob, decompressBlob } from "../compression/compressor.js";
import { StorageError } from "../types/storage-errors.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("sqlite-task-storage");

/**
 * SQLite Task Storage
 * Implementing the TaskStorageAdapter interface with metadata-BLOB separation
 */
export class SqliteTaskStorage
  extends BaseSqliteStorage<TaskStorageMetadata>
  implements TaskStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
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
  protected getBlobTableName(): string {
    return "task_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected createTableSchema(): void {
    const db = this.getDb();

    // Layer 1: Metadata table (frequent queries, no BLOB)
    db.exec(`
      CREATE TABLE IF NOT EXISTS task_metadata (
        id TEXT PRIMARY KEY,
        execution_id TEXT NOT NULL,
        workflow_id TEXT NOT NULL,
        status TEXT NOT NULL,
        submit_time INTEGER NOT NULL,
        start_time INTEGER,
        complete_time INTEGER,
        timeout INTEGER,
        execution_duration INTEGER,
        error TEXT CHECK(length(error) <= 4096),
        error_stack TEXT CHECK(length(error_stack) <= 8192),
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
      CREATE TABLE IF NOT EXISTS task_blob (
        task_id TEXT PRIMARY KEY,
        blob_data BLOB NOT NULL,
        compressed INTEGER DEFAULT 0 CHECK(compressed IN (0, 1)),
        compression_algorithm TEXT,
        CHECK(
          (compressed = 0 AND compression_algorithm IS NULL) OR
          (compressed = 1 AND compression_algorithm IS NOT NULL)
        ),
        FOREIGN KEY (task_id) REFERENCES task_metadata(id) ON DELETE CASCADE
      )
    `);

    // Create indexes for optimized queries
    db.exec(`CREATE INDEX IF NOT EXISTS idx_task_meta_execution_id ON task_metadata(execution_id)`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_task_meta_workflow_id ON task_metadata(workflow_id)`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_task_meta_status ON task_metadata(status)`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_task_meta_submit_time ON task_metadata(submit_time)`);
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_task_meta_complete_time ON task_metadata(complete_time)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_task_meta_status_submit ON task_metadata(status, submit_time)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_task_meta_created_at ON task_metadata(created_at)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_task_meta_updated_at ON task_metadata(updated_at)`,
    );
  }

  /**
   * Save task with metadata-BLOB separation and compression
   */
  async save(taskId: string, data: Uint8Array, metadata: TaskStorageMetadata): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      // Calculate execution duration if both start and complete times are available
      const executionDuration =
        metadata.completeTime && metadata.startTime
          ? metadata.completeTime - metadata.startTime
          : null;

      // Compute blob hash
      const blobHash = await this.computeHash(data);

      // Get adaptive compression config
      // Get compression config based on data characteristics
      const config = selectCompressionStrategy(data);

      // Compress BLOB data
      const { compressed, algorithm } = await compressBlob(data, config);

      const insertMetadata = db.prepare(`
        INSERT INTO task_metadata (
          id, execution_id, workflow_id, status, submit_time, start_time,
          complete_time, timeout, execution_duration, error, error_stack,
          blob_size, blob_hash, tags, custom_fields, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          execution_id = excluded.execution_id,
          workflow_id = excluded.workflow_id,
          status = excluded.status,
          submit_time = excluded.submit_time,
          start_time = excluded.start_time,
          complete_time = excluded.complete_time,
          timeout = excluded.timeout,
          execution_duration = excluded.execution_duration,
          error = excluded.error,
          error_stack = excluded.error_stack,
          blob_size = excluded.blob_size,
          blob_hash = excluded.blob_hash,
          tags = excluded.tags,
          custom_fields = excluded.custom_fields,
          updated_at = excluded.updated_at
      `);

      const insertBlob = db.prepare(`
        INSERT INTO task_blob (task_id, blob_data, compressed, compression_algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(task_id) DO UPDATE SET
          blob_data = excluded.blob_data,
          compressed = excluded.compressed,
          compression_algorithm = excluded.compression_algorithm
      `);

      db.transaction(() => {
        try {
          insertMetadata.run(
            taskId,
            metadata.executionId,
            metadata.workflowId,
            metadata.status,
            metadata.submitTime,
            metadata.startTime ?? null,
            metadata.completeTime ?? null,
            metadata.timeout ?? null,
            executionDuration,
            metadata.error ?? null,
            metadata.errorStack ?? null,
            compressed.length,
            blobHash,
            metadata.tags ? JSON.stringify(metadata.tags) : null,
            metadata.customFields ? JSON.stringify(metadata.customFields) : null,
            now,
            now,
          );

          insertBlob.run(taskId, compressed, algorithm ? 1 : 0, algorithm || null);
        } catch (error) {
          logger.error("Transaction failed during task save, rolling back", { 
            taskId, 
            error: (error as Error).message 
          });
          throw error; // Transaction will automatically rollback
        }
      })();
    } catch (error) {
      this.handleSqliteError(error, "save", { taskId });
    }
  }

  /**
   * Load task data with automatic decompression
   */
  override async load(id: string): Promise<Uint8Array | null> {
    const db = this.getDb();

    try {
      // Get both blob data and metadata for integrity verification
      const stmt = db.prepare(`
        SELECT tb.blob_data, tb.compressed, tb.compression_algorithm, tm.blob_hash
        FROM task_blob tb
        INNER JOIN task_metadata tm ON tb.task_id = tm.id
        WHERE tb.task_id = ?
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

      return finalData;
    } catch (error) {
      this.handleSqliteError(error, "load", { id });
    }
  }

  /**
   * Delete task (cascade delete will handle blob)
   */
  override async delete(id: string): Promise<void> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`DELETE FROM task_metadata WHERE id = ?`);
      stmt.run(id);
    } catch (error) {
      this.handleSqliteError(error, "delete", { id });
    }
  }

  /**
   * Check if task exists (only check metadata table)
   */
  override async exists(id: string): Promise<boolean> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT 1 FROM task_metadata WHERE id = ?`);
      const row = stmt.get(id);
      return row !== undefined;
    } catch (error) {
      this.handleSqliteError(error, "exists", { id });
    }
  }

  /**
   * List task IDs (optimized - only scans metadata table)
   */
  async list(options?: TaskListOptions): Promise<string[]> {
    const db = this.getDb();

    try {
      let sql = `SELECT id FROM task_metadata`;
      const params: unknown[] = [];
      const conditions: string[] = [];

      if (options?.executionId) {
        conditions.push("execution_id = ?");
        params.push(options.executionId);
      }

      if (options?.workflowId) {
        conditions.push("workflow_id = ?");
        params.push(options.workflowId);
      }

      if (options?.status) {
        if (Array.isArray(options.status)) {
          conditions.push(`status IN (${options.status.map(() => "?").join(", ")})`);
          params.push(...options.status);
        } else {
          conditions.push("status = ?");
          params.push(options.status);
        }
      }

      if (options?.submitTimeFrom) {
        conditions.push("submit_time >= ?");
        params.push(options.submitTimeFrom);
      }

      if (options?.submitTimeTo) {
        conditions.push("submit_time <= ?");
        params.push(options.submitTimeTo);
      }

      if (options?.startTimeFrom) {
        conditions.push("start_time >= ?");
        params.push(options.startTimeFrom);
      }

      if (options?.startTimeTo) {
        conditions.push("start_time <= ?");
        params.push(options.startTimeTo);
      }

      if (options?.completeTimeFrom) {
        conditions.push("complete_time >= ?");
        params.push(options.completeTimeFrom);
      }

      if (options?.completeTimeTo) {
        conditions.push("complete_time <= ?");
        params.push(options.completeTimeTo);
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

      const sortBy = options?.sortBy ?? "submitTime";
      const sortOrder = options?.sortOrder ?? "desc";
      // Convert camelCase to snake_case for SQL column names and validate strictly
      const allowedSortColumns = ['submit_time', 'start_time', 'complete_time', 'created_at', 'updated_at'];
      const sortColumnMap: Record<string, string> = {
        submitTime: 'submit_time',
        startTime: 'start_time',
        completeTime: 'complete_time',
        createdAt: 'created_at',
        updatedAt: 'updated_at',
      };
      const sortColumn = sortColumnMap[sortBy];
      
      if (!sortColumn || !allowedSortColumns.includes(sortColumn)) {
        throw new StorageError(`Invalid sort column: ${sortBy}. Allowed: ${allowedSortColumns.join(', ')}`, 'list', { sortBy });
      }
      
      // Use whitelist validation - never interpolate user input directly
      const orderDirection = sortOrder.toLowerCase() === 'asc' ? 'ASC' : 'DESC';
      sql += ` ORDER BY ${sortColumn} ${orderDirection}`;

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

      return rows.map(row => row.id);
    } catch (error) {
      this.handleSqliteError(error, "list", { options });
    }
  }

  /**
   * Get metadata (optimized - only reads metadata table)
   */
  async getMetadata(taskId: string): Promise<TaskStorageMetadata | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          id,
          execution_id as "executionId",
          workflow_id as "workflowId",
          status,
          submit_time as "submitTime",
          start_time as "startTime",
          complete_time as "completeTime",
          timeout,
          error,
          error_stack as "errorStack",
          tags,
          custom_fields as "customFields"
        FROM task_metadata WHERE id = ?
      `);
      const row = stmt.get(taskId) as
        | {
            id: string;
            executionId: string;
            workflowId: string;
            status: string;
            submitTime: number;
            startTime: number | null;
            completeTime: number | null;
            timeout: number | null;
            error: string | null;
            errorStack: string | null;
            tags: string | null;
            customFields: string | null;
          }
        | undefined;

      if (!row) {
        return null;
      }

      return {
        taskId: row.id,
        executionId: row.executionId,
        workflowId: row.workflowId,
        status: row.status as TaskStatus,
        submitTime: row.submitTime,
        startTime: row.startTime ?? undefined,
        completeTime: row.completeTime ?? undefined,
        timeout: row.timeout ?? undefined,
        error: row.error ?? undefined,
        errorStack: row.errorStack ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.customFields ? JSON.parse(row.customFields) : undefined,
      };
    } catch (error) {
      this.handleSqliteError(error, "getMetadata", { taskId });
    }
  }

  /**
   * Obtain task statistics information (optimized - only reads metadata table)
   */
  async getTaskStats(options?: TaskStatsOptions): Promise<TaskStats> {
    const db = this.getDb();

    try {
      let whereClause = "";
      const params: unknown[] = [];

      if (options?.workflowId) {
        whereClause = "WHERE workflow_id = ?";
        params.push(options.workflowId);
      }

      if (options?.timeFrom) {
        whereClause = whereClause
          ? `${whereClause} AND submit_time >= ?`
          : "WHERE submit_time >= ?";
        params.push(options.timeFrom);
      }

      if (options?.timeTo) {
        whereClause = whereClause
          ? `${whereClause} AND submit_time <= ?`
          : "WHERE submit_time <= ?";
        params.push(options.timeTo);
      }

      // Get the total count and status statistics.
      const countStmt = db.prepare(`
        SELECT
          COUNT(*) as total,
          SUM(CASE WHEN status = 'QUEUED' THEN 1 ELSE 0 END) as queued,
          SUM(CASE WHEN status = 'RUNNING' THEN 1 ELSE 0 END) as running,
          SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) as completed,
          SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) as failed,
          SUM(CASE WHEN status = 'CANCELLED' THEN 1 ELSE 0 END) as cancelled,
          SUM(CASE WHEN status = 'TIMEOUT' THEN 1 ELSE 0 END) as timeout
        FROM task_metadata ${whereClause}
      `);
      const countRow = countStmt.get(...params) as {
        total: number;
        queued: number;
        running: number;
        completed: number;
        failed: number;
        cancelled: number;
        timeout: number;
      };

      // Get statistics grouped by workflow
      const workflowStmt = db.prepare(`
        SELECT workflow_id, COUNT(*) as count
        FROM task_metadata ${whereClause}
        GROUP BY workflow_id
      `);
      const workflowRows = workflowStmt.all(...params) as Array<{
        workflow_id: string;
        count: number;
      }>;

      const byWorkflow: Record<string, number> = {};
      for (const row of workflowRows) {
        byWorkflow[row.workflow_id] = row.count;
      }

      // Calculate execution time statistics
      const timeWhereClause = whereClause
        ? `${whereClause} AND status = 'COMPLETED' AND start_time IS NOT NULL AND complete_time IS NOT NULL`
        : "WHERE status = 'COMPLETED' AND start_time IS NOT NULL AND complete_time IS NOT NULL";
      const timeStmt = db.prepare(`
        SELECT
          AVG(complete_time - start_time) as avg_time,
          MAX(complete_time - start_time) as max_time,
          MIN(complete_time - start_time) as min_time
        FROM task_metadata ${timeWhereClause}
      `);
      const timeRow = timeStmt.get(...params) as {
        avg_time: number | null;
        max_time: number | null;
        min_time: number | null;
      };

      const total = countRow.total || 0;
      const completed = countRow.completed || 0;
      const timeoutCount = countRow.timeout || 0;

      return {
        total,
        byStatus: {
          QUEUED: countRow.queued || 0,
          RUNNING: countRow.running || 0,
          COMPLETED: completed,
          FAILED: countRow.failed || 0,
          CANCELLED: countRow.cancelled || 0,
          TIMEOUT: timeoutCount,
        },
        byWorkflow,
        avgExecutionTime: timeRow.avg_time ?? undefined,
        maxExecutionTime: timeRow.max_time ?? undefined,
        minExecutionTime: timeRow.min_time ?? undefined,
        successRate: total > 0 ? completed / total : undefined,
        timeoutRate: total > 0 ? timeoutCount / total : undefined,
      };
    } catch (error) {
      this.handleSqliteError(error, "getStats", { options });
    }
  }

  /**
   * Clean up expired tasks
   */
  async cleanupTasks(retentionTime: number): Promise<number> {
    const db = this.getDb();
    const cutoffTime = Date.now() - retentionTime;

    try {
      const stmt = db.prepare(`
        DELETE FROM task_metadata
        WHERE status IN ('COMPLETED', 'FAILED', 'CANCELLED', 'TIMEOUT')
        AND complete_time IS NOT NULL
        AND complete_time < ?
      `);
      const result = stmt.run(cutoffTime);
      return result.changes;
    } catch (error) {
      this.handleSqliteError(error, "cleanup", { retentionTime });
    }
  }

  /**
   * Clear all tasks
   */
  override async clear(): Promise<void> {
    const db = this.getDb();

    try {
      db.exec(`DELETE FROM task_metadata`);
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
    }
  }
}
