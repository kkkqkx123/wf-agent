/**
 * PostgreSQL File Checkpoint Store Implementation
 * Stores file checkpoints in PostgreSQL database tables
 *
 * Design Note:
 * This storage directly implements FileCheckpointStorageAdapter instead of extending
 * StorageAdapterBase because file checkpoints have a different data model:
 * - File checkpoints store multiple files per checkpoint (Map<string, Buffer>)
 * - They don't fit the single metadata + blob pattern of other storage types
 * - The save() method accepts a Map of files instead of a single Uint8Array
 * This design choice is intentional and consistent across all FileCheckpointStore implementations.
 *
 * Database Schema:
 * - file_cp_metadata: stores checkpoint-level metadata
 * - file_cp_files: stores individual file content per checkpoint
 */

import { Pool } from "pg";
import { createModuleLogger } from "../logger.js";
import {
  PostgresConnectionPool,
  getGlobalConnectionPool,
  type PostgresPoolConfig,
} from "./connection-pool.js";
import type { FileCheckpointMetadata, FileCheckpointListOptions } from "@wf-agent/types";
import type { FileCheckpointStorageAdapter } from "../types/adapter/file-checkpoint-adapter.js";

const logger = createModuleLogger("postgres-file-checkpoint-store");

export interface PostgresFileCheckpointStoreConfig {
  /** PostgreSQL connection string */
  connectionString: string;
  /** Connection pool configuration */
  poolConfig?: PostgresPoolConfig;
  /** Use shared connection pool (default: true) */
  useConnectionPool?: boolean;
  /** Custom connection pool instance (optional) */
  connectionPool?: PostgresConnectionPool;
}

interface MetadataRow {
  id: string;
  entity_id: string;
  timestamp: number;
  type: string;
  base_checkpoint_id: string | null;
  file_count: number;
  empty_dirs: string;
  total_size: number;
  workspace_root: string;
  file_hash_snapshot: string;
  changes: string | null;
  created_at: Date;
}

export class PostgresFileCheckpointStore implements FileCheckpointStorageAdapter {
  private pool: Pool;
  private poolManager?: PostgresConnectionPool;
  private initialized = false;
  private config: PostgresFileCheckpointStoreConfig;

  constructor(config: PostgresFileCheckpointStoreConfig) {
    this.config = config;

    if (config.connectionPool) {
      this.poolManager = config.connectionPool;
      this.pool = this.poolManager.getPool(config.connectionString, config.poolConfig);
    } else if (config.useConnectionPool !== false) {
      this.poolManager = getGlobalConnectionPool();
      this.pool = this.poolManager.getPool(config.connectionString, config.poolConfig);
    } else {
      this.pool = new Pool({ connectionString: config.connectionString, ...config.poolConfig });
    }
  }

  private async getClient() {
    return this.pool.connect();
  }

  private rowToMetadata(row: MetadataRow): FileCheckpointMetadata {
    return {
      entityId: row.entity_id,
      timestamp: row.timestamp,
      type: row.type as "full" | "incremental",
      baseCheckpointId: row.base_checkpoint_id ?? undefined,
      fileCount: row.file_count,
      emptyDirs: JSON.parse(row.empty_dirs ?? "[]"),
      totalSize: row.total_size,
      workspaceRoot: row.workspace_root,
      fileHashSnapshot: JSON.parse(row.file_hash_snapshot),
      changes: row.changes ? JSON.parse(row.changes) : undefined,
    };
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;

    const client = await this.getClient();
    try {
      // Create metadata table
      await client.query(`
        CREATE TABLE IF NOT EXISTS file_cp_metadata (
          id TEXT PRIMARY KEY,
          entity_id TEXT NOT NULL,
          timestamp BIGINT NOT NULL,
          type TEXT NOT NULL CHECK(type IN ('full', 'incremental')),
          base_checkpoint_id TEXT,
          file_count INTEGER NOT NULL DEFAULT 0,
          empty_dirs JSONB DEFAULT '[]',
          total_size BIGINT NOT NULL DEFAULT 0,
          workspace_root TEXT NOT NULL,
          file_hash_snapshot JSONB NOT NULL,
          changes JSONB,
          created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
      `);

      // Create files table
      await client.query(`
        CREATE TABLE IF NOT EXISTS file_cp_files (
          checkpoint_id TEXT NOT NULL,
          path TEXT NOT NULL,
          data BYTEA,
          hash TEXT,
          is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
          PRIMARY KEY (checkpoint_id, path),
          FOREIGN KEY (checkpoint_id) REFERENCES file_cp_metadata(id) ON DELETE CASCADE
        )
      `);

      // Create indexes
      await client.query(`
        CREATE INDEX IF NOT EXISTS idx_fcp_entity_timestamp
        ON file_cp_metadata(entity_id, timestamp DESC)
      `);
      await client.query(`
        CREATE INDEX IF NOT EXISTS idx_fcp_base
        ON file_cp_metadata(base_checkpoint_id)
      `);

      this.initialized = true;
      logger.info("PostgresFileCheckpointStore initialized");
    } catch (error) {
      logger.error("Failed to initialize PostgresFileCheckpointStore", { error });
      throw error;
    } finally {
      client.release();
    }
  }

  async close(): Promise<void> {
    if (!this.config.connectionPool && this.config.useConnectionPool === false) {
      await this.pool.end();
    }
    this.initialized = false;
    logger.info("PostgresFileCheckpointStore closed");
  }

  async clear(): Promise<void> {
    const client = await this.getClient();
    try {
      await client.query("DELETE FROM file_cp_files");
      await client.query("DELETE FROM file_cp_metadata");
      logger.info("PostgresFileCheckpointStore cleared");
    } catch (error) {
      logger.error("Failed to clear store", { error });
      throw error;
    } finally {
      client.release();
    }
  }

  async save(
    id: string,
    metadata: FileCheckpointMetadata,
    files: Map<string, Buffer>,
  ): Promise<void> {
    if (!this.initialized) {
      await this.initialize();
    }

    const client = await this.getClient();
    try {
      await client.query("BEGIN");

      // Insert metadata
      await client.query(
        `INSERT INTO file_cp_metadata (
          id, entity_id, timestamp, type, base_checkpoint_id,
          file_count, empty_dirs, total_size, workspace_root,
          file_hash_snapshot, changes
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)`,
        [
          id,
          metadata.entityId,
          metadata.timestamp,
          metadata.type,
          metadata.baseCheckpointId ?? null,
          metadata.fileCount,
          JSON.stringify(metadata.emptyDirs ?? []),
          metadata.totalSize,
          metadata.workspaceRoot,
          JSON.stringify(metadata.fileHashSnapshot ?? {}),
          metadata.changes ? JSON.stringify(metadata.changes) : null,
        ],
      );

      // Insert files
      const insertFileQuery = `
        INSERT INTO file_cp_files (checkpoint_id, path, data, hash, is_deleted)
        VALUES ($1, $2, $3, $4, $5)
      `;

      for (const [filePath, content] of files) {
        const hash = metadata.fileHashSnapshot?.[filePath] ?? "";
        await client.query(insertFileQuery, [id, filePath, content, hash, false]);
      }

      // Record deleted files in incremental checkpoints
      if (metadata.changes) {
        for (const change of metadata.changes) {
          if (change.type === "deleted") {
            await client.query(insertFileQuery, [
              id,
              change.path,
              null,
              change.hash,
              true,
            ]);
          }
        }
      }

      await client.query("COMMIT");
      logger.debug("File checkpoint saved", { checkpointId: id, fileCount: files.size });
    } catch (error) {
      await client.query("ROLLBACK");
      logger.error("Failed to save file checkpoint", { checkpointId: id, error });
      throw error;
    } finally {
      client.release();
    }
  }

  async load(id: string): Promise<{ metadata: FileCheckpointMetadata; files: Map<string, Buffer> } | null> {
    const client = await this.getClient();
    try {
      const metaResult = await client.query(
        "SELECT * FROM file_cp_metadata WHERE id = $1",
        [id],
      );

      if (metaResult.rows.length === 0) return null;

      const metadata = this.rowToMetadata(metaResult.rows[0] as MetadataRow);

      const filesResult = await client.query(
        "SELECT path, data FROM file_cp_files WHERE checkpoint_id = $1 AND is_deleted = FALSE AND data IS NOT NULL",
        [id],
      );

      const files = new Map<string, Buffer>();
      for (const row of filesResult.rows) {
        if (row.data) {
          files.set(row.path, row.data);
        }
      }

      return { metadata, files };
    } catch (error) {
      logger.error("Failed to load file checkpoint", { checkpointId: id, error });
      throw error;
    } finally {
      client.release();
    }
  }

  async delete(id: string): Promise<void> {
    const client = await this.getClient();
    try {
      await client.query("BEGIN");
      await client.query("DELETE FROM file_cp_files WHERE checkpoint_id = $1", [id]);
      await client.query("DELETE FROM file_cp_metadata WHERE id = $1", [id]);
      await client.query("COMMIT");
      logger.debug("File checkpoint deleted", { checkpointId: id });
    } catch (error) {
      await client.query("ROLLBACK");
      logger.error("Failed to delete file checkpoint", { checkpointId: id, error });
      throw error;
    } finally {
      client.release();
    }
  }

  async list(options?: FileCheckpointListOptions): Promise<string[]> {
    const client = await this.getClient();
    try {
      let sql = "SELECT id FROM file_cp_metadata WHERE 1=1";
      const params: unknown[] = [];
      let paramIndex = 1;

      if (options?.entityId) {
        sql += ` AND entity_id = $${paramIndex++}`;
        params.push(options.entityId);
      }
      if (options?.type) {
        sql += ` AND type = $${paramIndex++}`;
        params.push(options.type);
      }
      if (options?.timestampFrom !== undefined) {
        sql += ` AND timestamp >= $${paramIndex++}`;
        params.push(options.timestampFrom);
      }
      if (options?.timestampTo !== undefined) {
        sql += ` AND timestamp <= $${paramIndex++}`;
        params.push(options.timestampTo);
      }

      sql += " ORDER BY timestamp DESC";

      if (options?.limit) {
        sql += ` LIMIT $${paramIndex++}`;
        params.push(options.limit);
      }
      if (options?.offset) {
        sql += ` OFFSET $${paramIndex++}`;
        params.push(options.offset);
      }

      const result = await client.query(sql, params);
      return result.rows.map((r: { id: string }) => r.id);
    } catch (error) {
      logger.error("Failed to list file checkpoints", { error });
      throw error;
    } finally {
      client.release();
    }
  }

  async listByEntity(
    entityId: string,
    options?: { limit?: number },
  ): Promise<Array<{ id: string; metadata: FileCheckpointMetadata }>> {
    const client = await this.getClient();
    try {
      let sql = "SELECT * FROM file_cp_metadata WHERE entity_id = $1 ORDER BY timestamp DESC";
      const params: unknown[] = [entityId];

      if (options?.limit) {
        sql += " LIMIT $2";
        params.push(options.limit);
      }

      const result = await client.query(sql, params);
      return result.rows.map((row: MetadataRow) => ({
        id: row.id,
        metadata: this.rowToMetadata(row),
      }));
    } catch (error) {
      logger.error("Failed to list checkpoints by entity", { entityId, error });
      throw error;
    } finally {
      client.release();
    }
  }

  async getLatestByEntity(
    entityId: string,
  ): Promise<{ id: string; metadata: FileCheckpointMetadata; files?: Map<string, Buffer> } | null> {
    const items = await this.listByEntity(entityId, { limit: 1 });
    if (items.length === 0) return null;

    const item = items[0]!;
    const loaded = await this.load(item.id);
    return {
      ...item,
      files: loaded?.files,
    };
  }

  async deleteByEntity(entityId: string, keepLatest?: number): Promise<number> {
    const client = await this.getClient();
    try {
      await client.query("BEGIN");

      let deletedCount: number;

      if (keepLatest && keepLatest > 0) {
        // Get IDs to keep
        const keepResult = await client.query(
          "SELECT id FROM file_cp_metadata WHERE entity_id = $1 ORDER BY timestamp DESC LIMIT $2",
          [entityId, keepLatest],
        );
        const keepIds = new Set(keepResult.rows.map((r: { id: string }) => r.id));

        // Get all IDs for entity
        const allResult = await client.query(
          "SELECT id FROM file_cp_metadata WHERE entity_id = $1",
          [entityId],
        );

        const deleteIds = allResult.rows
          .filter((r: { id: string }) => !keepIds.has(r.id))
          .map((r: { id: string }) => r.id);

        for (const id of deleteIds) {
          await client.query("DELETE FROM file_cp_files WHERE checkpoint_id = $1", [id]);
          await client.query("DELETE FROM file_cp_metadata WHERE id = $1", [id]);
        }

        deletedCount = deleteIds.length;
      } else {
        await client.query(
          "DELETE FROM file_cp_files WHERE checkpoint_id IN (SELECT id FROM file_cp_metadata WHERE entity_id = $1)",
          [entityId],
        );
        const metaResult = await client.query(
          "DELETE FROM file_cp_metadata WHERE entity_id = $1",
          [entityId],
        );
        deletedCount = metaResult.rowCount ?? 0;
      }

      await client.query("COMMIT");

      logger.info("Deleted file checkpoints by entity", {
        entityId,
        count: deletedCount,
      });

      return deletedCount;
    } catch (error) {
      await client.query("ROLLBACK");
      logger.error("Failed to delete checkpoints by entity", { entityId, error });
      throw error;
    } finally {
      client.release();
    }
  }
}
