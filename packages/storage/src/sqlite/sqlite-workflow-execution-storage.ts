/**
 * SQLite Workflow Execution Storage Implementation with Metadata-BLOB Separation
 * Workflow execution persistent storage based on better-sqlite3
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
  WorkflowExecutionType,
} from "@wf-agent/types";
import type { WorkflowExecutionStorageAdapter } from "../types/adapter/workflow-execution-adapter.js";
import { BaseSqliteStorage, BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
import { selectCompressionStrategy } from "../compression/adaptive-compression.js";
import { compressBlob, decompressBlob } from "../compression/compressor.js";
import { StorageError } from "../types/storage-errors.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("sqlite-workflow-execution-storage");

/**
 * SQLite Workflow Execution Storage
 * Implementing the WorkflowExecutionStorageAdapter interface with metadata-BLOB separation
 */
export class SqliteWorkflowExecutionStorage
  extends BaseSqliteStorage<WorkflowExecutionStorageMetadata>
  implements WorkflowExecutionStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
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
  protected getBlobTableName(): string {
    return "workflow_execution_blob";
  }


  /**
   * Create table structure with metadata-BLOB separation
   */
  protected createTableSchema(): void {
    const db = this.getDb();

    // Layer 1: Metadata table (frequent queries, no BLOB)
    db.exec(`
      CREATE TABLE IF NOT EXISTS workflow_execution_metadata (
        id TEXT PRIMARY KEY,
        workflow_id TEXT NOT NULL,
        workflow_version TEXT NOT NULL,
        status TEXT NOT NULL,
        execution_type TEXT,
        current_node_id TEXT,
        parent_execution_id TEXT,
        start_time INTEGER NOT NULL,
        end_time INTEGER,
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
      CREATE TABLE IF NOT EXISTS workflow_execution_blob (
        execution_id TEXT PRIMARY KEY,
        blob_data BLOB NOT NULL,
        compressed INTEGER DEFAULT 0 CHECK(compressed IN (0, 1)),
        compression_algorithm TEXT,
        CHECK(
          (compressed = 0 AND compression_algorithm IS NULL) OR
          (compressed = 1 AND compression_algorithm IS NOT NULL)
        ),
        FOREIGN KEY (execution_id) REFERENCES workflow_execution_metadata(id) ON DELETE CASCADE
      )
    `);

    // Create indexes for optimized queries
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_workflow_id ON workflow_execution_metadata(workflow_id)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_status ON workflow_execution_metadata(status)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_start_time ON workflow_execution_metadata(start_time)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_end_time ON workflow_execution_metadata(end_time)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_parent ON workflow_execution_metadata(parent_execution_id)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_workflow_status ON workflow_execution_metadata(workflow_id, status)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_wf_exec_meta_workflow_start ON workflow_execution_metadata(workflow_id, start_time)`,
    );
  }

  /**
   * Save workflow execution with metadata-BLOB separation and compression
   */
  async save(id: string, data: Uint8Array, metadata: WorkflowExecutionStorageMetadata): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      // Get adaptive compression config
      // Get compression config based on data characteristics
      const config = selectCompressionStrategy(data);

      // Compress BLOB data
      const { compressed, algorithm } = await compressBlob(data, config);
      
      // Compute blob hash
      const blobHash = await this.computeHash(data);

      // Use transaction to ensure atomicity
      const insertMetadata = db.prepare(`
        INSERT INTO workflow_execution_metadata (
          id, workflow_id, workflow_version, status, execution_type,
          current_node_id, parent_execution_id, start_time, end_time,
          blob_size, blob_hash, tags, custom_fields, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          workflow_id = excluded.workflow_id,
          workflow_version = excluded.workflow_version,
          status = excluded.status,
          execution_type = excluded.execution_type,
          current_node_id = excluded.current_node_id,
          parent_execution_id = excluded.parent_execution_id,
          start_time = excluded.start_time,
          end_time = excluded.end_time,
          blob_size = excluded.blob_size,
          blob_hash = excluded.blob_hash,
          tags = excluded.tags,
          custom_fields = excluded.custom_fields,
          updated_at = excluded.updated_at
      `);

      const insertBlob = db.prepare(`
        INSERT INTO workflow_execution_blob (execution_id, blob_data, compressed, compression_algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(execution_id) DO UPDATE SET
          blob_data = excluded.blob_data,
          compressed = excluded.compressed,
          compression_algorithm = excluded.compression_algorithm
      `);

      db.transaction(() => {
        try {
          insertMetadata.run(
            id,
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
            now,
            now,
          );

          insertBlob.run(id, compressed, algorithm ? 1 : 0, algorithm || null);
        } catch (error) {
          logger.error("Transaction failed during workflow execution save, rolling back", { 
            id, 
            error: (error as Error).message 
          });
          throw error; // Transaction will automatically rollback
        }
      })();
    } catch (error) {
      this.handleSqliteError(error, "save", { id });
    }
  }

  /**
   * Load workflow execution data with automatic decompression
   */
  override async load(id: string): Promise<Uint8Array | null> {
    const db = this.getDb();

    try {
      // Get both blob data and metadata for integrity verification
      const stmt = db.prepare(`
        SELECT web.blob_data, web.compressed, web.compression_algorithm, wem.blob_hash
        FROM workflow_execution_blob web
        INNER JOIN workflow_execution_metadata wem ON web.execution_id = wem.id
        WHERE web.execution_id = ?
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
   * Delete workflow execution (cascade delete will handle blob)
   */
  override async delete(id: string): Promise<void> {
    const db = this.getDb();

    try {
      // Due to ON DELETE CASCADE, deleting from metadata will also delete from blob table
      const stmt = db.prepare(`DELETE FROM workflow_execution_metadata WHERE id = ?`);
      stmt.run(id);
    } catch (error) {
      this.handleSqliteError(error, "delete", { id });
    }
  }

  /**
   * Check if workflow execution exists (only check metadata table)
   */
  override async exists(id: string): Promise<boolean> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT 1 FROM workflow_execution_metadata WHERE id = ?`);
      const row = stmt.get(id);
      return row !== undefined;
    } catch (error) {
      this.handleSqliteError(error, "exists", { id });
    }
  }

  /**
   * List workflow execution IDs (optimized - only scans metadata table)
   */
  async list(options?: WorkflowExecutionListOptions): Promise<string[]> {
    const db = this.getDb();

    try {
      let sql = `SELECT id FROM workflow_execution_metadata`;
      const params: unknown[] = [];
      const conditions: string[] = [];

      // Construct filter criteria
      if (options?.workflowId) {
        conditions.push("workflow_id = ?");
        params.push(options.workflowId);
      }

      if (options?.status) {
        if (Array.isArray(options.status)) {
          const placeholders = options.status.map(() => "?").join(", ");
          conditions.push(`status IN (${placeholders})`);
          params.push(...options.status);
        } else {
          conditions.push("status = ?");
          params.push(options.status);
        }
      }

      if (options?.executionType) {
        conditions.push("execution_type = ?");
        params.push(options.executionType);
      }

      if (options?.parentExecutionId) {
        conditions.push("parent_execution_id = ?");
        params.push(options.parentExecutionId);
      }

      if (options?.startTimeFrom) {
        conditions.push("start_time >= ?");
        params.push(options.startTimeFrom);
      }

      if (options?.startTimeTo) {
        conditions.push("start_time <= ?");
        params.push(options.startTimeTo);
      }

      if (options?.endTimeFrom) {
        conditions.push("(end_time IS NOT NULL AND end_time >= ?)");
        params.push(options.endTimeFrom);
      }

      if (options?.endTimeTo) {
        conditions.push("(end_time IS NOT NULL AND end_time <= ?)");
        params.push(options.endTimeTo);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags LIKE ?`);
        params.push(`%${options.tags[0]}%`);
      }

      if (conditions.length > 0) {
        sql += " WHERE " + conditions.join(" AND ");
      }

      // Sort
      const sortBy = options?.sortBy ?? "startTime";
      const sortOrder = options?.sortOrder ?? "desc";
      
      switch (sortBy) {
        case "startTime":
          sql += ` ORDER BY start_time ${sortOrder === "asc" ? "ASC" : "DESC"}`;
          break;
        case "endTime":
          sql += ` ORDER BY end_time ${sortOrder === "asc" ? "ASC" : "DESC"} NULLS LAST`;
          break;
        case "updatedAt":
        default:
          sql += ` ORDER BY updated_at ${sortOrder === "asc" ? "ASC" : "DESC"}`;
          break;
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

      return rows.map(row => row.id);
    } catch (error) {
      this.handleSqliteError(error, "list", { options });
    }
  }

  /**
   * Get metadata (optimized - only reads metadata table)
   */
  async getMetadata(id: string): Promise<WorkflowExecutionStorageMetadata | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          workflow_id as "workflowId",
          workflow_version as "workflowVersion",
          status,
          execution_type as "executionType",
          current_node_id as "currentNodeId",
          parent_execution_id as "parentExecutionId",
          start_time as "startTime",
          end_time as "endTime",
          tags,
          custom_fields as "customFields"
        FROM workflow_execution_metadata WHERE id = ?
      `);
      const row = stmt.get(id) as
        | {
            workflowId: string;
            workflowVersion: string;
            status: WorkflowExecutionStatus;
            executionType: string | null;
            currentNodeId: string | null;
            parentExecutionId: string | null;
            startTime: number;
            endTime: number | null;
            tags: string | null;
            customFields: string | null;
          }
        | undefined;

      if (!row) {
        return null;
      }

      return {
        executionId: id,
        workflowId: row.workflowId,
        workflowVersion: row.workflowVersion,
        status: row.status,
        executionType: (row.executionType as WorkflowExecutionType) ?? undefined,
        currentNodeId: row.currentNodeId ?? undefined,
        parentExecutionId: row.parentExecutionId ?? undefined,
        startTime: row.startTime,
        endTime: row.endTime ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.customFields ? JSON.parse(row.customFields) : undefined,
      };
    } catch (error) {
      this.handleSqliteError(error, "getMetadata", { id });
    }
  }

  /**
   * Update workflow execution status
   * Only updates metadata table, no need to touch BLOB data
   */
  async updateExecutionStatus(executionId: string, status: WorkflowExecutionStatus): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      // First check if execution exists
      const exists = await this.exists(executionId);
      if (!exists) {
        throw new StorageError(`Workflow execution not found: ${executionId}`, "updateStatus", { executionId });
      }

      // Update only the status and timestamps in metadata
      const stmt = db.prepare(`
        UPDATE workflow_execution_metadata 
        SET status = ?, updated_at = ?, end_time = CASE WHEN ? IN ('COMPLETED', 'FAILED', 'CANCELLED') THEN ? ELSE end_time END
        WHERE id = ?
      `);
      
      const endTime = ['COMPLETED', 'FAILED', 'CANCELLED'].includes(status) ? now : null;
      stmt.run(status, now, status, endTime, executionId);
    } catch (error) {
      if (error instanceof StorageError) {
        throw error;
      }
      this.handleSqliteError(error, "updateExecutionStatus", { executionId, status });
    }
  }

  /**
   * Clear all workflow executions
   */
  override async clear(): Promise<void> {
    const db = this.getDb();

    try {
      // Due to ON DELETE CASCADE, clearing metadata will also clear blob table
      db.exec(`DELETE FROM workflow_execution_metadata`);
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
    }
  }

  /**
   * Get storage statistics
   */
  async getStats(): Promise<{
    totalCount: number;
    byStatus: Record<WorkflowExecutionStatus, number>;
    byType: Record<string, number>;
  }> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          COUNT(*) as total_count,
          status,
          execution_type,
          COUNT(*) as count
        FROM workflow_execution_metadata
        GROUP BY status, execution_type
      `);
      const rows = stmt.all() as Array<{
        total_count: number;
        status: WorkflowExecutionStatus;
        execution_type: string | null;
        count: number;
      }>;

      const byStatus: Record<WorkflowExecutionStatus, number> = {} as any;
      const byType: Record<string, number> = {};

      for (const row of rows) {
        byStatus[row.status] = (byStatus[row.status] || 0) + row.count;
        if (row.execution_type) {
          byType[row.execution_type] = (byType[row.execution_type] || 0) + row.count;
        }
      }

      return {
        totalCount: rows.reduce((sum, row) => sum + row.count, 0),
        byStatus,
        byType,
      };
    } catch (error) {
      this.handleSqliteError(error, "getStats", {});
    }
  }
}
