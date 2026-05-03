/**
 * SQLite Agent Loop Checkpoint Storage Implementation with Metadata-BLOB Separation
 * Agent loop checkpoint persistent storage based on better-sqlite3
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type {
  AgentCheckpointMetadata,
  AgentCheckpointListOptions,
  TCheckpointType,
} from "@wf-agent/types";
import type { AgentLoopCheckpointStorageAdapter } from "../types/adapter/index.js";
import { BaseSqliteStorage, BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
import { selectCompressionStrategy } from "../compression/adaptive-compression.js";
import { compressBlob, decompressBlob } from "../compression/compressor.js";

/**
 * SQLite Agent Loop Checkpoint Storage
 * Implementing the AgentLoopCheckpointStorageAdapter interface with metadata-BLOB separation
 */
export class SqliteAgentLoopCheckpointStorage
  extends BaseSqliteStorage<AgentCheckpointMetadata>
  implements AgentLoopCheckpointStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
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
  protected getBlobTableName(): string {
    return "agent_loop_checkpoint_blob";
  }


  /**
   * Create table structure with metadata-BLOB separation
   */
  protected createTableSchema(): void {
    const db = this.getDb();

    // Layer 1: Metadata table (frequent queries, no BLOB)
    db.exec(`
      CREATE TABLE IF NOT EXISTS agent_loop_checkpoint_metadata (
        id TEXT PRIMARY KEY,
        agent_loop_id TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        type TEXT NOT NULL,
        version INTEGER,
        blob_size INTEGER,
        blob_hash TEXT,
        tags TEXT CHECK(length(tags) <= 4096),
        custom_fields TEXT CHECK(length(custom_fields) <= 8192),
        created_at INTEGER NOT NULL
      )
    `);

    // Layer 2: BLOB storage table (infrequent direct access)
    db.exec(`
      CREATE TABLE IF NOT EXISTS agent_loop_checkpoint_blob (
        checkpoint_id TEXT PRIMARY KEY,
        blob_data BLOB NOT NULL,
        compressed INTEGER DEFAULT 0 CHECK(compressed IN (0, 1)),
        compression_algorithm TEXT,
        CHECK(
          (compressed = 0 AND compression_algorithm IS NULL) OR
          (compressed = 1 AND compression_algorithm IS NOT NULL)
        ),
        FOREIGN KEY (checkpoint_id) REFERENCES agent_loop_checkpoint_metadata(id) ON DELETE CASCADE
      )
    `);

    // Create indexes for optimized queries
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_agent_cp_meta_agent_loop_id ON agent_loop_checkpoint_metadata(agent_loop_id)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_agent_cp_meta_timestamp ON agent_loop_checkpoint_metadata(timestamp)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_agent_cp_meta_type ON agent_loop_checkpoint_metadata(type)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_agent_cp_meta_agent_timestamp ON agent_loop_checkpoint_metadata(agent_loop_id, timestamp)`,
    );
  }

  /**
   * Save agent loop checkpoint with metadata-BLOB separation and compression
   */
  async save(checkpointId: string, data: Uint8Array, metadata: AgentCheckpointMetadata): Promise<void> {
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

      const insertMetadata = db.prepare(`
        INSERT INTO agent_loop_checkpoint_metadata (
          id, agent_loop_id, timestamp, type, version,
          blob_size, blob_hash, tags, custom_fields, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          agent_loop_id = excluded.agent_loop_id,
          timestamp = excluded.timestamp,
          type = excluded.type,
          version = excluded.version,
          blob_size = excluded.blob_size,
          blob_hash = excluded.blob_hash,
          tags = excluded.tags,
          custom_fields = excluded.custom_fields,
          created_at = excluded.created_at
      `);

      const insertBlob = db.prepare(`
        INSERT INTO agent_loop_checkpoint_blob (checkpoint_id, blob_data, compressed, compression_algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(checkpoint_id) DO UPDATE SET
          blob_data = excluded.blob_data,
          compressed = excluded.compressed,
          compression_algorithm = excluded.compression_algorithm
      `);

      db.transaction(() => {
        insertMetadata.run(
          checkpointId,
          metadata.agentLoopId,
          metadata.timestamp,
          metadata.type,
          metadata.version ?? null,
          compressed.length,
          blobHash,
          metadata.tags ? JSON.stringify(metadata.tags) : null,
          metadata.customFields ? JSON.stringify(metadata.customFields) : null,
          now,
        );

        insertBlob.run(checkpointId, compressed, algorithm ? 1 : 0, algorithm || null);
      })();
    } catch (error) {
      this.handleSqliteError(error, "save", { checkpointId });
    }
  }

  /**
   * Load agent loop checkpoint data with automatic decompression
   */
  override async load(id: string): Promise<Uint8Array | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT blob_data, compressed, compression_algorithm
        FROM agent_loop_checkpoint_blob
        WHERE checkpoint_id = ?
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
   * Delete agent loop checkpoint (cascade delete will handle blob)
   */
  override async delete(id: string): Promise<void> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`DELETE FROM agent_loop_checkpoint_metadata WHERE id = ?`);
      stmt.run(id);
    } catch (error) {
      this.handleSqliteError(error, "delete", { id });
    }
  }

  /**
   * Check if agent loop checkpoint exists (only check metadata table)
   */
  override async exists(id: string): Promise<boolean> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT 1 FROM agent_loop_checkpoint_metadata WHERE id = ?`);
      const row = stmt.get(id);
      return row !== undefined;
    } catch (error) {
      this.handleSqliteError(error, "exists", { id });
    }
  }

  /**
   * List agent loop checkpoint IDs (optimized - only scans metadata table)
   */
  async list(options?: AgentCheckpointListOptions): Promise<string[]> {
    const db = this.getDb();

    try {
      let sql = `SELECT id FROM agent_loop_checkpoint_metadata`;
      const params: unknown[] = [];
      const conditions: string[] = [];

      if (options?.agentLoopId) {
        conditions.push("agent_loop_id = ?");
        params.push(options.agentLoopId);
      }

      if (options?.type) {
        conditions.push("type = ?");
        params.push(options.type);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags LIKE ?`);
        params.push(`%"${options.tags[0]}"%`);
      }

      if (conditions.length > 0) {
        sql += " WHERE " + conditions.join(" AND ");
      }

      sql += ` ORDER BY timestamp DESC`;

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
  async getMetadata(checkpointId: string): Promise<AgentCheckpointMetadata | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          agent_loop_id as "agentLoopId",
          timestamp,
          type,
          version,
          tags,
          custom_fields as "customFields"
        FROM agent_loop_checkpoint_metadata WHERE id = ?
      `);
      const row = stmt.get(checkpointId) as
        | {
            agentLoopId: string;
            timestamp: number;
            type: string;
            version: number | null;
            tags: string | null;
            customFields: string | null;
          }
        | undefined;

      if (!row) {
        return null;
      }

      return {
        agentLoopId: row.agentLoopId,
        timestamp: row.timestamp,
        type: row.type as TCheckpointType,
        version: row.version ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.customFields ? JSON.parse(row.customFields) : undefined,
      };
    } catch (error) {
      this.handleSqliteError(error, "getMetadata", { checkpointId });
    }
  }

  /**
   * List checkpoints for a specific agent loop
   */
  async listByAgentLoop(
    agentLoopId: string,
    options?: Omit<AgentCheckpointListOptions, "agentLoopId">,
  ): Promise<string[]> {
    const db = this.getDb();
  
    try {
      let sql = `SELECT id FROM agent_loop_checkpoint_metadata WHERE agent_loop_id = ?`;
      const params: unknown[] = [agentLoopId];
      const conditions: string[] = [];
  
      if (options?.type) {
        conditions.push("type = ?");
        params.push(options.type);
      }
  
      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags LIKE ?`);
        params.push(`%${options.tags[0]}%`);
      }

      if (conditions.length > 0) {
        sql += " AND " + conditions.join(" AND ");
      }

      sql += ` ORDER BY timestamp DESC`;

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
      this.handleSqliteError(error, "listByAgentLoop", { agentLoopId, options });
    }
  }

  /**
   * Get the latest checkpoint ID for an agent loop
   */
  async getLatestCheckpoint(agentLoopId: string): Promise<string | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT id FROM agent_loop_checkpoint_metadata
        WHERE agent_loop_id = ?
        ORDER BY timestamp DESC
        LIMIT 1
      `);
      const row = stmt.get(agentLoopId) as { id: string } | undefined;

      return row ? row.id : null;
    } catch (error) {
      this.handleSqliteError(error, "getLatestCheckpoint", { agentLoopId });
    }
  }

  /**
   * Delete all checkpoints for an agent loop
   */
  async deleteByAgentLoop(agentLoopId: string): Promise<number> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        DELETE FROM agent_loop_checkpoint_metadata
        WHERE agent_loop_id = ?
      `);
      const result = stmt.run(agentLoopId);
      return result.changes;
    } catch (error) {
      this.handleSqliteError(error, "deleteByAgentLoop", { agentLoopId });
    }
  }

  /**
   * Clear all agent loop checkpoints
   */
  override async clear(): Promise<void> {
    const db = this.getDb();

    try {
      db.exec(`DELETE FROM agent_loop_checkpoint_metadata`);
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
    }
  }
}
