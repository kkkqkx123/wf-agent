/**
 * SQLite Thread Storage Implementation with Metadata-BLOB Separation
 * Thread persistent storage based on better-sqlite3
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type { ThreadStorageMetadata, ThreadListOptions, ThreadStatus } from "@wf-agent/types";
import type { ThreadStorageCallback } from "../types/callback/index.js";
import { BaseSqliteStorage, BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
import { compressBlob, decompressBlob } from "./compression.js";

/**
 * SQLite Thread Storage
 * Implements the ThreadStorageCallback interface with metadata-BLOB separation
 */
export class SqliteThreadStorage
  extends BaseSqliteStorage<ThreadStorageMetadata>
  implements ThreadStorageCallback
{
  constructor(config: BaseSqliteStorageConfig) {
    super(config);
  }

  /**
   * Get metadata table name
   */
  protected getTableName(): string {
    return "thread_metadata";
  }

  /**
   * Get BLOB table name
   */
  protected getBlobTableName(): string {
    return "thread_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected createTableSchema(): void {
    const db = this.getDb();

    // Layer 1: Metadata table (frequent queries, no BLOB)
    db.exec(`
      CREATE TABLE IF NOT EXISTS thread_metadata (
        id TEXT PRIMARY KEY,
        workflow_id TEXT NOT NULL,
        workflow_version TEXT NOT NULL,
        status TEXT NOT NULL,
        thread_type TEXT,
        current_node_id TEXT,
        parent_thread_id TEXT,
        start_time INTEGER NOT NULL,
        end_time INTEGER,
        execution_duration INTEGER,
        checkpoint_count INTEGER DEFAULT 0,
        blob_size INTEGER,
        blob_hash TEXT,
        tags TEXT,
        custom_fields TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      )
    `);

    // Layer 2: BLOB storage table (infrequent direct access)
    db.exec(`
      CREATE TABLE IF NOT EXISTS thread_blob (
        thread_id TEXT PRIMARY KEY,
        blob_data BLOB NOT NULL,
        compressed BOOLEAN DEFAULT FALSE,
        compression_algorithm TEXT,
        FOREIGN KEY (thread_id) REFERENCES thread_metadata(id) ON DELETE CASCADE
      )
    `);

    // Create indexes for optimized queries
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_thread_meta_workflow_id ON thread_metadata(workflow_id)`,
    );
    db.exec(`CREATE INDEX IF NOT EXISTS idx_thread_meta_status ON thread_metadata(status)`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_thread_meta_start_time ON thread_metadata(start_time)`);
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_thread_meta_parent_thread_id ON thread_metadata(parent_thread_id)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_thread_meta_workflow_status ON thread_metadata(workflow_id, status)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_thread_meta_status_start ON thread_metadata(status, start_time)`,
    );
  }

  /**
   * Compute simple hash for BLOB data
   */
  private computeHash(data: Uint8Array): string {
    let hash = 0x811c9dc5;
    for (let i = 0; i < data.length; i++) {
      hash ^= data[i]!;
      hash += (hash << 1) + (hash << 4) + (hash << 7) + (hash << 8) + (hash << 24);
    }
    return (hash >>> 0).toString(16).padStart(8, "0");
  }

  /**
   * Save thread with metadata-BLOB separation and compression
   */
  async save(id: string, data: Uint8Array, metadata: ThreadStorageMetadata): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      // Calculate execution duration if end time is available
      const executionDuration =
        metadata.endTime && metadata.startTime ? metadata.endTime - metadata.startTime : null;

      // Compute blob hash
      const blobHash = this.computeHash(data);

      // Compress BLOB data
      const { compressed, algorithm } = await compressBlob(data);

      const insertMetadata = db.prepare(`
        INSERT INTO thread_metadata (
          id, workflow_id, workflow_version, status, thread_type,
          current_node_id, parent_thread_id, start_time, end_time,
          execution_duration, blob_size, blob_hash, tags, custom_fields, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          workflow_id = excluded.workflow_id,
          workflow_version = excluded.workflow_version,
          status = excluded.status,
          thread_type = excluded.thread_type,
          current_node_id = excluded.current_node_id,
          parent_thread_id = excluded.parent_thread_id,
          start_time = excluded.start_time,
          end_time = excluded.end_time,
          execution_duration = excluded.execution_duration,
          blob_size = excluded.blob_size,
          blob_hash = excluded.blob_hash,
          tags = excluded.tags,
          custom_fields = excluded.custom_fields,
          updated_at = excluded.updated_at
      `);

      const insertBlob = db.prepare(`
        INSERT INTO thread_blob (thread_id, blob_data, compressed, compression_algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(thread_id) DO UPDATE SET
          blob_data = excluded.blob_data,
          compressed = excluded.compressed,
          compression_algorithm = excluded.compression_algorithm
      `);

      db.transaction(() => {
        insertMetadata.run(
          id,
          metadata.workflowId,
          metadata.workflowVersion,
          metadata.status,
          metadata.threadType ?? null,
          metadata.currentNodeId ?? null,
          metadata.parentThreadId ?? null,
          metadata.startTime,
          metadata.endTime ?? null,
          executionDuration,
          compressed.length,
          blobHash,
          metadata.tags ? JSON.stringify(metadata.tags) : null,
          metadata.customFields ? JSON.stringify(metadata.customFields) : null,
          now,
          now,
        );

        insertBlob.run(id, Buffer.from(compressed), algorithm ? 1 : 0, algorithm);
      })();
    } catch (error) {
      this.handleSqliteError(error, "save", { id });
    }
  }

  /**
   * Load thread data with automatic decompression
   */
  override async load(id: string): Promise<Uint8Array | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT blob_data, compressed, compression_algorithm
        FROM thread_blob
        WHERE thread_id = ?
      `);
      const row = stmt.get(id) as
        | {
            blob_data: Buffer;
            compressed: number;
            compression_algorithm: string | null;
          }
        | undefined;

      if (!row) {
        return null;
      }

      const data = new Uint8Array(row.blob_data);

      // Decompress if needed
      if (row.compressed && row.compression_algorithm) {
        return await decompressBlob(data, row.compression_algorithm);
      }

      return data;
    } catch (error) {
      this.handleSqliteError(error, "load", { id });
    }
  }

  /**
   * Delete thread (cascade delete will handle blob)
   */
  override async delete(id: string): Promise<void> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`DELETE FROM thread_metadata WHERE id = ?`);
      stmt.run(id);
    } catch (error) {
      this.handleSqliteError(error, "delete", { id });
    }
  }

  /**
   * Check if thread exists (only check metadata table)
   */
  override async exists(id: string): Promise<boolean> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT 1 FROM thread_metadata WHERE id = ?`);
      const row = stmt.get(id);
      return row !== undefined;
    } catch (error) {
      this.handleSqliteError(error, "exists", { id });
    }
  }

  /**
   * List thread IDs (optimized - only scans metadata table)
   */
  async list(options?: ThreadListOptions): Promise<string[]> {
    const db = this.getDb();

    try {
      let sql = `SELECT id FROM thread_metadata`;
      const params: unknown[] = [];
      const conditions: string[] = [];

      // Constructing Filter Criteria
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

      if (options?.threadType) {
        conditions.push("thread_type = ?");
        params.push(options.threadType);
      }

      if (options?.parentThreadId) {
        conditions.push("parent_thread_id = ?");
        params.push(options.parentThreadId);
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
        conditions.push("end_time >= ?");
        params.push(options.endTimeFrom);
      }

      if (options?.endTimeTo) {
        conditions.push("end_time <= ?");
        params.push(options.endTimeTo);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags LIKE ?`);
        params.push(`%"${options.tags[0]}"%`);
      }

      if (conditions.length > 0) {
        sql += " WHERE " + conditions.join(" AND ");
      }

      // arrange in order
      const sortBy = options?.sortBy ?? "startTime";
      const sortOrder = options?.sortOrder ?? "desc";
      // Convert camelCase to snake_case for SQL column names
      const sortColumn =
        sortBy === "startTime" ? "start_time" : sortBy === "endTime" ? "end_time" : sortBy;
      sql += ` ORDER BY ${sortColumn} ${sortOrder.toUpperCase()}`;

      // subdivision
      if (options?.limit !== undefined) {
        sql += " LIMIT ?";
        params.push(options.limit);
      }

      if (options?.offset !== undefined) {
        sql += " OFFSET ?";
        params.push(options.offset);
      }

      const stmt = db.prepare(sql);
      const rows = stmt.all(...params) as Array<{ id: string }>;

      return rows.map(row => row.id);
    } catch (error) {
      this.handleSqliteError(error, "list", { options });
    }
  }

  /**
   * Getting Metadata (optimized - only reads metadata table)
   */
  async getMetadata(id: string): Promise<ThreadStorageMetadata | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          id,
          workflow_id as "workflowId",
          workflow_version as "workflowVersion",
          status,
          thread_type as "threadType",
          current_node_id as "currentNodeId",
          parent_thread_id as "parentThreadId",
          start_time as "startTime",
          end_time as "endTime",
          tags,
          custom_fields as "customFields"
        FROM thread_metadata WHERE id = ?
      `);
      const row = stmt.get(id) as
        | {
            id: string;
            workflowId: string;
            workflowVersion: string;
            status: string;
            threadType: string | null;
            currentNodeId: string | null;
            parentThreadId: string | null;
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
        threadId: row.id,
        workflowId: row.workflowId,
        workflowVersion: row.workflowVersion,
        status: row.status as ThreadStatus,
        threadType: row.threadType as import("@wf-agent/types").ThreadType | undefined,
        currentNodeId: row.currentNodeId ?? undefined,
        parentThreadId: row.parentThreadId ?? undefined,
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
   * Update Thread Status (optimized - only updates metadata table)
   */
  async updateThreadStatus(threadId: string, status: ThreadStatus): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      const stmt = db.prepare(`
        UPDATE thread_metadata SET status = ?, updated_at = ? WHERE id = ?
      `);
      stmt.run(status, now, threadId);
    } catch (error) {
      this.handleSqliteError(error, "updateStatus", { threadId, status });
    }
  }

  /**
   * Clear all threads
   */
  override async clear(): Promise<void> {
    const db = this.getDb();

    try {
      db.exec(`DELETE FROM thread_metadata`);
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
    }
  }
}
