/**
 * SQLite Workflow Storage Implementation with Metadata-BLOB Separation
 * Workflow persistence storage based on better-sqlite3
 *
 * Optimized Design:
 * - Metadata and BLOB data stored in separate tables for better query performance
 * - BLOB compression support to reduce storage space
 * - List queries only scan metadata table, avoiding BLOB reads
 */

import type {
  WorkflowStorageMetadata,
  WorkflowListOptions,
  WorkflowVersionInfo,
  WorkflowVersionListOptions,
} from "@wf-agent/types";
import type { WorkflowStorageAdapter } from "../types/adapter/index.js";
import { BaseSqliteStorage, BaseSqliteStorageConfig } from "./base-sqlite-storage.js";
import { CompressionService } from "../compression/compression-service.js";
import { compressBlob, decompressBlob } from "../compression/compressor.js";
import { StorageError } from "../types/storage-errors.js";

/**
 * SQLite Workflow Storage
 * Implementing the WorkflowStorageAdapter interface with metadata-BLOB separation
 */
export class SqliteWorkflowStorage
  extends BaseSqliteStorage<WorkflowStorageMetadata>
  implements WorkflowStorageAdapter
{
  constructor(config: BaseSqliteStorageConfig) {
    super(config);
  }

  /**
   * Get metadata table name
   */
  protected getTableName(): string {
    return "workflow_metadata";
  }

  /**
   * Get BLOB table name
   */
  protected getBlobTableName(): string {
    return "workflow_blob";
  }

  /**
   * Get version metadata table name
   */
  protected getVersionMetadataTableName(): string {
    return "workflow_version_metadata";
  }

  /**
   * Get version BLOB table name
   */
  protected getVersionBlobTableName(): string {
    return "workflow_version_blob";
  }

  /**
   * Create table structure with metadata-BLOB separation
   */
  protected createTableSchema(): void {
    const db = this.getDb();

    // Layer 1: Workflow metadata table (frequent queries, no BLOB)
    db.exec(`
      CREATE TABLE IF NOT EXISTS workflow_metadata (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        version TEXT NOT NULL,
        description TEXT CHECK(length(description) <= 2048),
        author TEXT CHECK(length(author) <= 256),
        category TEXT CHECK(length(category) <= 128),
        tags TEXT CHECK(length(tags) <= 4096),
        enabled INTEGER DEFAULT 1,
        node_count INTEGER NOT NULL,
        edge_count INTEGER NOT NULL,
        blob_size INTEGER,
        blob_hash TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        custom_fields TEXT CHECK(length(custom_fields) <= 8192)
      )
    `);

    // Layer 2: Workflow BLOB storage table (infrequent direct access)
    db.exec(`
      CREATE TABLE IF NOT EXISTS workflow_blob (
        workflow_id TEXT PRIMARY KEY,
        blob_data BLOB NOT NULL,
        compressed INTEGER DEFAULT 0 CHECK(compressed IN (0, 1)),
        compression_algorithm TEXT,
        CHECK(
          (compressed = 0 AND compression_algorithm IS NULL) OR
          (compressed = 1 AND compression_algorithm IS NOT NULL)
        ),
        FOREIGN KEY (workflow_id) REFERENCES workflow_metadata(id) ON DELETE CASCADE
      )
    `);

    // Layer 1: Version metadata table
    db.exec(`
      CREATE TABLE IF NOT EXISTS workflow_version_metadata (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        workflow_id TEXT NOT NULL,
        version TEXT NOT NULL,
        change_note TEXT CHECK(length(change_note) <= 2048),
        blob_size INTEGER,
        created_at INTEGER NOT NULL,
        created_by TEXT CHECK(length(created_by) <= 256),
        UNIQUE(workflow_id, version),
        FOREIGN KEY (workflow_id) REFERENCES workflow_metadata(id) ON DELETE CASCADE
      )
    `);

    // Layer 2: Version BLOB storage table
    db.exec(`
      CREATE TABLE IF NOT EXISTS workflow_version_blob (
        version_id INTEGER PRIMARY KEY,
        blob_data BLOB NOT NULL,
        compressed INTEGER DEFAULT 0 CHECK(compressed IN (0, 1)),
        compression_algorithm TEXT,
        CHECK(
          (compressed = 0 AND compression_algorithm IS NULL) OR
          (compressed = 1 AND compression_algorithm IS NOT NULL)
        ),
        FOREIGN KEY (version_id) REFERENCES workflow_version_metadata(id) ON DELETE CASCADE
      )
    `);

    // Create indexes for optimized queries
    db.exec(`CREATE INDEX IF NOT EXISTS idx_workflow_meta_name ON workflow_metadata(name)`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_workflow_meta_category ON workflow_metadata(category)`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_workflow_meta_author ON workflow_metadata(author)`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_workflow_meta_enabled ON workflow_metadata(enabled)`);
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_workflow_meta_created_at ON workflow_metadata(created_at)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_workflow_meta_updated_at ON workflow_metadata(updated_at)`,
    );
    db.exec(
      `CREATE INDEX IF NOT EXISTS idx_workflow_version_meta_workflow_id ON workflow_version_metadata(workflow_id)`,
    );
  }


  /**
   * Save workflow with metadata-BLOB separation and compression
   */
  async save(
    workflowId: string,
    data: Uint8Array,
    metadata: WorkflowStorageMetadata,
  ): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      // Compute blob hash
      const blobHash = await this.computeHash(data);

      // Get adaptive compression config
      const service = CompressionService.getInstance();
      const config = service.getAdaptiveConfig(data, 'workflow');

      // Compress BLOB data
      const { compressed, algorithm } = await compressBlob(data, config);

      const insertMetadata = db.prepare(`
        INSERT INTO workflow_metadata (
          id, name, version, description, author, category, tags, enabled,
          node_count, edge_count, blob_size, blob_hash, created_at, updated_at, custom_fields
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          name = excluded.name,
          version = excluded.version,
          description = excluded.description,
          author = excluded.author,
          category = excluded.category,
          tags = excluded.tags,
          enabled = excluded.enabled,
          node_count = excluded.node_count,
          edge_count = excluded.edge_count,
          blob_size = excluded.blob_size,
          blob_hash = excluded.blob_hash,
          updated_at = excluded.updated_at,
          custom_fields = excluded.custom_fields
      `);

      const insertBlob = db.prepare(`
        INSERT INTO workflow_blob (workflow_id, blob_data, compressed, compression_algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(workflow_id) DO UPDATE SET
          blob_data = excluded.blob_data,
          compressed = excluded.compressed,
          compression_algorithm = excluded.compression_algorithm
      `);

      db.transaction(() => {
        insertMetadata.run(
          workflowId,
          metadata.name,
          metadata.version,
          metadata.description ?? null,
          metadata.author ?? null,
          metadata.category ?? null,
          metadata.tags ? JSON.stringify(metadata.tags) : null,
          metadata.enabled !== false ? 1 : 0,
          metadata.nodeCount,
          metadata.edgeCount,
          compressed.length,
          blobHash,
          now,
          now,
          metadata.customFields ? JSON.stringify(metadata.customFields) : null,
        );

        insertBlob.run(workflowId, compressed, algorithm ? 1 : 0, algorithm || null);
      })();
    } catch (error) {
      this.handleSqliteError(error, "save", { workflowId });
    }
  }

  /**
   * Load workflow data with automatic decompression
   */
  override async load(id: string): Promise<Uint8Array | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT blob_data, compressed, compression_algorithm
        FROM workflow_blob
        WHERE workflow_id = ?
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
   * Delete workflow (cascade delete will handle blobs and versions)
   */
  override async delete(workflowId: string): Promise<void> {
    const db = this.getDb();

    try {
      // Due to ON DELETE CASCADE, deleting from metadata will also delete from blob and version tables
      const stmt = db.prepare(`DELETE FROM workflow_metadata WHERE id = ?`);
      stmt.run(workflowId);
    } catch (error) {
      this.handleSqliteError(error, "delete", { workflowId });
    }
  }

  /**
   * Check if workflow exists (only check metadata table)
   */
  override async exists(id: string): Promise<boolean> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`SELECT 1 FROM workflow_metadata WHERE id = ?`);
      const row = stmt.get(id);
      return row !== undefined;
    } catch (error) {
      this.handleSqliteError(error, "exists", { id });
    }
  }

  /**
   * List workflow IDs (optimized - only scans metadata table)
   */
  async list(options?: WorkflowListOptions): Promise<string[]> {
    const db = this.getDb();

    try {
      let sql = `SELECT id FROM workflow_metadata`;
      const params: unknown[] = [];
      const conditions: string[] = [];

      if (options?.name) {
        conditions.push("name LIKE ?");
        params.push(`%${options.name}%`);
      }

      if (options?.author) {
        conditions.push("author = ?");
        params.push(options.author);
      }

      if (options?.category) {
        conditions.push("category = ?");
        params.push(options.category);
      }

      if (options?.enabled !== undefined) {
        conditions.push("enabled = ?");
        params.push(options.enabled ? 1 : 0);
      }

      if (options?.createdAtFrom) {
        conditions.push("created_at >= ?");
        params.push(options.createdAtFrom);
      }

      if (options?.createdAtTo) {
        conditions.push("created_at <= ?");
        params.push(options.createdAtTo);
      }

      if (options?.updatedAtFrom) {
        conditions.push("updated_at >= ?");
        params.push(options.updatedAtFrom);
      }

      if (options?.updatedAtTo) {
        conditions.push("updated_at <= ?");
        params.push(options.updatedAtTo);
      }

      if (options?.tags && options.tags.length > 0) {
        conditions.push(`tags LIKE ?`);
        params.push(`%${options.tags[0]}%`);
      }

      if (conditions.length > 0) {
        sql += " WHERE " + conditions.join(" AND ");
      }

      const sortBy = options?.sortBy ?? "updatedAt";
      const sortOrder = options?.sortOrder ?? "desc";
      // Convert camelCase to snake_case for SQL column names and validate
      const allowedSortColumns = ['updated_at', 'created_at', 'name'];
      const sortColumnMap: Record<string, string> = {
        updatedAt: 'updated_at',
        createdAt: 'created_at',
        name: 'name',
      };
      const sortColumn = sortColumnMap[sortBy] || sortBy;
      
      if (!allowedSortColumns.includes(sortColumn)) {
        throw new StorageError(`Invalid sort column: ${sortBy}`, 'list', { sortBy });
      }
      
      sql += ` ORDER BY ${sortColumn} ${sortOrder.toUpperCase()}`;

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
  async getMetadata(workflowId: string): Promise<WorkflowStorageMetadata | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT
          id,
          name,
          version,
          description,
          author,
          category,
          tags,
          enabled,
          node_count as "nodeCount",
          edge_count as "edgeCount",
          created_at as "createdAt",
          updated_at as "updatedAt",
          custom_fields as "customFields"
        FROM workflow_metadata WHERE id = ?
      `);
      const row = stmt.get(workflowId) as
        | {
            id: string;
            name: string;
            version: string;
            description: string | null;
            author: string | null;
            category: string | null;
            tags: string | null;
            enabled: number;
            nodeCount: number;
            edgeCount: number;
            createdAt: number;
            updatedAt: number;
            customFields: string | null;
          }
        | undefined;

      if (!row) {
        return null;
      }

      return {
        workflowId: row.id,
        name: row.name,
        version: row.version,
        description: row.description ?? undefined,
        author: row.author ?? undefined,
        category: row.category ?? undefined,
        tags: row.tags ? JSON.parse(row.tags) : undefined,
        enabled: row.enabled === 1,
        nodeCount: row.nodeCount,
        edgeCount: row.edgeCount,
        createdAt: row.createdAt,
        updatedAt: row.updatedAt,
        customFields: row.customFields ? JSON.parse(row.customFields) : undefined,
      };
    } catch (error) {
      this.handleSqliteError(error, "getMetadata", { workflowId });
    }
  }

  /**
   * Update workflow metadata (optimized - only updates metadata table)
   */
  async updateWorkflowMetadata(
    workflowId: string,
    metadata: Partial<WorkflowStorageMetadata>,
  ): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      const updates: string[] = ["updated_at = ?"];
      const params: unknown[] = [now];

      if (metadata.name !== undefined) {
        updates.push("name = ?");
        params.push(metadata.name);
      }
      if (metadata.description !== undefined) {
        updates.push("description = ?");
        params.push(metadata.description);
      }
      if (metadata.author !== undefined) {
        updates.push("author = ?");
        params.push(metadata.author);
      }
      if (metadata.category !== undefined) {
        updates.push("category = ?");
        params.push(metadata.category);
      }
      if (metadata.tags !== undefined) {
        updates.push("tags = ?");
        params.push(JSON.stringify(metadata.tags));
      }
      if (metadata.enabled !== undefined) {
        updates.push("enabled = ?");
        params.push(metadata.enabled ? 1 : 0);
      }
      if (metadata.customFields !== undefined) {
        updates.push("custom_fields = ?");
        params.push(JSON.stringify(metadata.customFields));
      }

      params.push(workflowId);

      const stmt = db.prepare(`UPDATE workflow_metadata SET ${updates.join(", ")} WHERE id = ?`);
      stmt.run(...params);
    } catch (error) {
      this.handleSqliteError(error, "updateMetadata", { workflowId });
    }
  }

  // ==================== Version Control ====================

  /**
   * Save workflow version with metadata-BLOB separation
   */
  async saveWorkflowVersion(
    workflowId: string,
    version: string,
    data: Uint8Array,
    changeNote?: string,
  ): Promise<void> {
    const db = this.getDb();
    const now = Date.now();

    try {
      // Get adaptive compression config for version
      const service = CompressionService.getInstance();
      const config = service.getAdaptiveConfig(data, 'workflow');

      // Compress BLOB data
      const { compressed, algorithm } = await compressBlob(data, config);

      const insertMetadata = db.prepare(`
        INSERT INTO workflow_version_metadata (
          workflow_id, version, change_note, blob_size, created_at
        )
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(workflow_id, version) DO UPDATE SET
          change_note = excluded.change_note,
          blob_size = excluded.blob_size,
          created_at = excluded.created_at
        RETURNING id
      `);

      const insertBlob = db.prepare(`
        INSERT INTO workflow_version_blob (version_id, blob_data, compressed, compression_algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(version_id) DO UPDATE SET
          blob_data = excluded.blob_data,
          compressed = excluded.compressed,
          compression_algorithm = excluded.compression_algorithm
      `);

      db.transaction(() => {
        // Insert metadata and get the version_id
        const result = insertMetadata.get(
          workflowId,
          version,
          changeNote ?? null,
          compressed.length,
          now
        ) as { id: number } | undefined;

        if (!result) {
          throw new Error('Failed to insert version metadata');
        }

        // Insert blob using the returned version_id
        insertBlob.run(result.id, compressed, algorithm ? 1 : 0, algorithm || null);
      })();
    } catch (error) {
      this.handleSqliteError(error, "saveVersion", { workflowId, version });
    }
  }

  /**
   * List workflow versions (optimized - only reads metadata table)
   */
  async listWorkflowVersions(
    workflowId: string,
    options?: WorkflowVersionListOptions,
  ): Promise<WorkflowVersionInfo[]> {
    const db = this.getDb();

    try {
      let sql = `
        SELECT
          v.id,
          v.version,
          v.created_at as "createdAt",
          v.created_by as "createdBy",
          v.change_note as "changeNote"
        FROM workflow_version_metadata v
        WHERE v.workflow_id = ?
        ORDER BY v.created_at DESC
      `;
      const params: unknown[] = [workflowId];

      if (options?.limit !== undefined) {
        sql += " LIMIT ?";
        params.push(options.limit);
      }

      if (options?.offset !== undefined) {
        sql += " OFFSET ?";
        params.push(options.offset);
      }

      const stmt = db.prepare(sql);
      const rows = stmt.all(...params) as Array<{
        id: number;
        version: string;
        createdAt: number;
        createdBy: string | null;
        changeNote: string | null;
      }>;

      // Get the current version
      const currentWorkflow = db
        .prepare(`SELECT version FROM workflow_metadata WHERE id = ?`)
        .get(workflowId) as { version: string } | undefined;
      const currentVersion = currentWorkflow?.version;

      return rows.map(row => ({
        version: row.version,
        createdAt: row.createdAt,
        createdBy: row.createdBy ?? undefined,
        changeNote: row.changeNote ?? undefined,
        isCurrent: row.version === currentVersion,
      }));
    } catch (error) {
      this.handleSqliteError(error, "listVersions", { workflowId });
    }
  }

  /**
   * Load workflow version with automatic decompression
   */
  async loadWorkflowVersion(workflowId: string, version: string): Promise<Uint8Array | null> {
    const db = this.getDb();

    try {
      const stmt = db.prepare(`
        SELECT vb.blob_data, vb.compressed, vb.compression_algorithm
        FROM workflow_version_blob vb
        JOIN workflow_version_metadata vm ON vb.version_id = vm.id
        WHERE vm.workflow_id = ? AND vm.version = ?
      `);
      const row = stmt.get(workflowId, version) as
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
      this.handleSqliteError(error, "loadVersion", { workflowId, version });
    }
  }

  /**
   * Delete workflow version
   */
  async deleteWorkflowVersion(workflowId: string, version: string): Promise<void> {
    const db = this.getDb();

    try {
      // Due to ON DELETE CASCADE, deleting from metadata will also delete from blob table
      const stmt = db.prepare(`
        DELETE FROM workflow_version_metadata WHERE workflow_id = ? AND version = ?
      `);
      stmt.run(workflowId, version);
    } catch (error) {
      this.handleSqliteError(error, "deleteVersion", { workflowId, version });
    }
  }

  /**
   * Clear all workflows (cascade delete will handle blobs and versions)
   */
  override async clear(): Promise<void> {
    const db = this.getDb();

    try {
      // Due to ON DELETE CASCADE, clearing metadata will also clear blob and version tables
      db.exec(`DELETE FROM workflow_metadata`);
    } catch (error) {
      this.handleSqliteError(error, "clear", {});
    }
  }
}
