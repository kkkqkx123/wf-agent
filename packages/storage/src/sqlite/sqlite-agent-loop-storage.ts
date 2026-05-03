/**
 * SQLite Agent Loop Storage Implementation with Metadata-BLOB Separation
 * Agent loop entity persistent storage based on better-sqlite3
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type {
  AgentEntityMetadata,
  AgentEntityListOptions,
  AgentLoopStatus,
} from "@wf-agent/types";
import type { AgentLoopStorageAdapter } from "../types/adapter/index.js";
import { BaseSqliteStorage, BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
import { CompressionService } from "../compression/compression-service.js";
import { compressBlob, decompressBlob } from "../compression/compressor.js";

/**
 * SQLite Agent Loop Storage
 * Implementing the AgentLoopStorageAdapter interface with metadata-BLOB separation
 */
export class SqliteAgentLoopStorage
  extends BaseSqliteStorage<AgentEntityMetadata>
  implements AgentLoopStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
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
  protected getBlobTableName(): string {
    return "agent_loop_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected createTableSchema(): void {
    const db = this.getDb();

    // Layer 1: Metadata table (frequent queries, no BLOB)
    db.exec(`
      CREATE TABLE IF NOT EXISTS agent_loop_metadata (
        id TEXT PRIMARY KEY,
        status TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER,
        completed_at INTEGER,
        profile_id TEXT,
        tags TEXT,
        custom_fields TEXT
      )
    `);

    // Layer 2: BLOB storage table (infrequent direct access)
    db.exec(`
      CREATE TABLE IF NOT EXISTS agent_loop_blob (
        agent_loop_id TEXT PRIMARY KEY,
        blob_data BLOB NOT NULL,
        compressed BOOLEAN DEFAULT FALSE,
        compression_algorithm TEXT,
        FOREIGN KEY (agent_loop_id) REFERENCES agent_loop_metadata(id) ON DELETE CASCADE
      )
    `);

    // Create indexes for optimized queries
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_agent_loop_meta_status ON agent_loop_metadata(status)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_agent_loop_meta_profile_id ON agent_loop_metadata(profile_id)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_agent_loop_meta_created_at ON agent_loop_metadata(created_at)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_agent_loop_meta_status_created ON agent_loop_metadata(status, created_at)`,
    );
  }

  /**
   * Save agent loop with metadata-BLOB separation and compression
   */
  async save(agentLoopId: string, data: Uint8Array, metadata: AgentEntityMetadata): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      // Get adaptive compression config
      const service = CompressionService.getInstance();
      const config = service.getAdaptiveConfig(data, 'agentLoop');

      // Compress BLOB data
      const { compressed, algorithm } = await compressBlob(data, config);

      const insertMetadata = db.prepare(`
        INSERT INTO agent_loop_metadata (
          id, status, created_at, updated_at, completed_at,
          profile_id, tags, custom_fields
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          status = excluded.status,
          updated_at = excluded.updated_at,
          completed_at = excluded.completed_at,
          profile_id = excluded.profile_id,
          tags = excluded.tags,
          custom_fields = excluded.custom_fields
      `);

      const insertBlob = db.prepare(`
        INSERT INTO agent_loop_blob (agent_loop_id, blob_data, compressed, compression_algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(agent_loop_id) DO UPDATE SET
          blob_data = excluded.blob_data,
          compressed = excluded.compressed,
          compression_algorithm = excluded.compression_algorithm
      `);

      db.transaction(() => {
        insertMetadata.run(
          agentLoopId,
          metadata.status,
          metadata.createdAt,
          metadata.updatedAt ?? null,
          metadata.completedAt ?? null,
          metadata.profileId ?? null,
          metadata.tags ? JSON.stringify(metadata.tags) : null,
          metadata.customFields ? JSON.stringify(metadata.customFields) : null,
        );

        insertBlob.run(agentLoopId, Buffer.from(compressed), algorithm ? 1 : 0, algorithm);
      })();
    } catch (error) {
      this.handleSqliteError(error, "save", { agentLoopId });
    }
  }

  /**
   * Load agent loop data with automatic decompression
   */
  override async load(id: string): Promise<Uint8Array | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT blob_data, compressed, compression_algorithm
        FROM agent_loop_blob
        WHERE agent_loop_id = ?
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
   * Delete agent loop (cascade delete will handle blob)
   */
  override async delete(id: string): Promise<void> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`DELETE FROM agent_loop_metadata WHERE id = ?`);
      stmt.run(id);
    } catch (error) {
      this.handleSqliteError(error, "delete", { id });
    }
  }

  /**
   * Check if agent loop exists (only check metadata table)
   */
  override async exists(id: string): Promise<boolean> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT 1 FROM agent_loop_metadata WHERE id = ?`);
      const row = stmt.get(id);
      return row !== undefined;
    } catch (error) {
      this.handleSqliteError(error, "exists", { id });
    }
  }

  /**
   * List agent loop IDs (optimized - only scans metadata table)
   */
  async list(options?: AgentEntityListOptions): Promise<string[]> {
    const db = this.getDb();

    try {
      let sql = `SELECT id FROM agent_loop_metadata`;
      const params: unknown[] = [];
      const conditions: string[] = [];

      if (options?.status) {
        conditions.push("status = ?");
        params.push(options.status);
      }

      if (options?.profileId) {
        conditions.push("profile_id = ?");
        params.push(options.profileId);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags LIKE ?`);
        params.push(`%${options.tags[0]}%`);
      }

      if (options?.createdAfter) {
        conditions.push("created_at >= ?");
        params.push(options.createdAfter);
      }

      if (options?.createdBefore) {
        conditions.push("created_at <= ?");
        params.push(options.createdBefore);
      }

      if (conditions.length > 0) {
        sql += " WHERE " + conditions.join(" AND ");
      }

      sql += ` ORDER BY created_at DESC`;

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
   * Get metadata (optimized - only reads metadata table)
   */
  async getMetadata(agentLoopId: string): Promise<AgentEntityMetadata | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          status,
          created_at as "createdAt",
          updated_at as "updatedAt",
          completed_at as "completedAt",
          profile_id as "profileId",
          tags,
          custom_fields as "customFields"
        FROM agent_loop_metadata WHERE id = ?
      `);
      const row = stmt.get(agentLoopId) as
        | {
            status: string;
            createdAt: number;
            updatedAt: number | null;
            completedAt: number | null;
            profileId: string | null;
            tags: string | null;
            customFields: string | null;
          }
        | undefined;

      if (!row) {
        return null;
      }

      return {
        agentLoopId,
        status: row.status as AgentLoopStatus,
        createdAt: row.createdAt,
        updatedAt: row.updatedAt ?? undefined,
        completedAt: row.completedAt ?? undefined,
        profileId: row.profileId ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        customFields: row.customFields ? JSON.parse(row.customFields) : undefined,
      };
    } catch (error) {
      this.handleSqliteError(error, "getMetadata", { agentLoopId });
    }
  }

  /**
   * Update agent loop status
   * Only updates metadata table, no need to touch BLOB data
   */
  async updateAgentLoopStatus(agentLoopId: string, status: AgentLoopStatus): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      // First check if agent loop exists
      const exists = await this.exists(agentLoopId);
      if (!exists) {
        throw new Error(`Agent loop not found: ${agentLoopId}`);
      }

      // Update only the status and timestamps in metadata
      const stmt = db.prepare(`
        UPDATE agent_loop_metadata 
        SET status = ?, updated_at = ?, completed_at = CASE WHEN ? IN ('COMPLETED', 'FAILED', 'CANCELLED') THEN ? ELSE completed_at END
        WHERE id = ?
      `);

      const completedAt = ["COMPLETED", "FAILED", "CANCELLED"].includes(status) ? now : null;
      stmt.run(status, now, status, completedAt, agentLoopId);
    } catch (error) {
      if (error instanceof Error && error.message.includes("not found")) {
        throw error;
      }
      this.handleSqliteError(error, "updateAgentLoopStatus", { agentLoopId, status });
    }
  }

  /**
   * List agent loops by status
   */
  async listByStatus(status: AgentLoopStatus): Promise<string[]> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT id FROM agent_loop_metadata
        WHERE status = ?
        ORDER BY created_at DESC
      `);
      const rows = stmt.all(status) as Array<{ id: string }>;

      return rows.map(row => row.id);
    } catch (error) {
      this.handleSqliteError(error, "listByStatus", { status });
    }
  }

  /**
   * Get agent loop statistics
   */
  async getAgentLoopStats(): Promise<{
    total: number;
    byStatus: Record<string, number>;
  }> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          COUNT(*) as total,
          status,
          COUNT(*) as count
        FROM agent_loop_metadata
        GROUP BY status
      `);
      const rows = stmt.all() as Array<{
        total: number;
        status: string;
        count: number;
      }>;

      const byStatus: Record<string, number> = {};
      let total = 0;

      for (const row of rows) {
        byStatus[row.status] = row.count;
        total += row.count;
      }

      return {
        total,
        byStatus,
      };
    } catch (error) {
      this.handleSqliteError(error, "getAgentLoopStats", {});
    }
  }

  /**
   * Clear all agent loops
   */
  override async clear(): Promise<void> {
    const db = this.getDb();

    try {
      db.exec(`DELETE FROM agent_loop_metadata`);
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
    }
  }
}
