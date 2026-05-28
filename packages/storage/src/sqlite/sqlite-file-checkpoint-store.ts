/**
 * SQLite File Checkpoint Store Implementation
 *
 * Stores file checkpoint metadata and file contents using SQLite.
 * Uses two tables:
 * - file_cp_metadata: checkpoint metadata and hash snapshots
 * - file_cp_files: actual file content stored per checkpoint
 *
 * Design follows the same metadata-BLOB separation pattern as other storage implementations.
 */
import Database from 'better-sqlite3';
import { createModuleLogger } from '../logger.js';
import type { FileCheckpointMetadata, FileCheckpointListOptions } from '@wf-agent/types';
import type { FileCheckpointStorageAdapter } from '../types/adapter/file-checkpoint-adapter.js';

const logger = createModuleLogger('sqlite-file-checkpoint-store');

/**
 * SQLite file checkpoint store configuration
 */
export interface SqliteFileCheckpointStoreConfig {
  /** Path to SQLite database file */
  dbPath: string;
  /** Enable WAL mode for better concurrent performance */
  enableWAL?: boolean;
}

/**
 * SQLite file checkpoint storage implementation
 *
 * Schema:
 * - file_cp_metadata: stores checkpoint-level metadata as TEXT columns
 * - file_cp_files: stores individual file content per checkpoint
 */
export class SqliteFileCheckpointStore implements FileCheckpointStorageAdapter {
  private db!: Database.Database;
  private config: SqliteFileCheckpointStoreConfig;
  private initialized = false;

  constructor(config: SqliteFileCheckpointStoreConfig) {
    this.config = config;
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;

    this.db = new Database(this.config.dbPath);

    if (this.config.enableWAL !== false) {
      this.db.pragma('journal_mode = WAL');
    }

    this.createSchema();
    this.initialized = true;
    logger.info('SqliteFileCheckpointStore initialized', { dbPath: this.config.dbPath });
  }

  async close(): Promise<void> {
    if (!this.initialized) return;
    this.db.close();
    this.initialized = false;
    logger.info('SqliteFileCheckpointStore closed');
  }

  async clear(): Promise<void> {
    this.db.exec('DELETE FROM file_cp_files');
    this.db.exec('DELETE FROM file_cp_metadata');
    logger.info('SqliteFileCheckpointStore cleared');
  }

  async save(
    id: string,
    metadata: FileCheckpointMetadata,
    files: Map<string, Buffer>,
  ): Promise<void> {
    const insertMetadata = this.db.prepare(`
      INSERT INTO file_cp_metadata (
        id, entity_id, timestamp, type, base_checkpoint_id,
        file_count, empty_dirs, total_size, workspace_root,
        file_hash_snapshot, changes, created_at
      ) VALUES (
        @id, @entityId, @timestamp, @type, @baseCheckpointId,
        @fileCount, @emptyDirs, @totalSize, @workspaceRoot,
        @fileHashSnapshot, @changes, @createdAt
      )
    `);

    const insertFile = this.db.prepare(`
      INSERT INTO file_cp_files (checkpoint_id, path, data, hash, is_deleted)
      VALUES (@checkpointId, @path, @data, @hash, @isDeleted)
    `);

    const transaction = this.db.transaction(() => {
      insertMetadata.run({
        id,
        entityId: metadata.entityId,
        timestamp: metadata.timestamp,
        type: metadata.type,
        baseCheckpointId: metadata.baseCheckpointId ?? null,
        fileCount: metadata.fileCount,
        emptyDirs: JSON.stringify(metadata.emptyDirs),
        totalSize: metadata.totalSize,
        workspaceRoot: metadata.workspaceRoot,
        fileHashSnapshot: JSON.stringify(metadata.fileHashSnapshot),
        changes: metadata.changes ? JSON.stringify(metadata.changes) : null,
        createdAt: Date.now(),
      });

      for (const [filePath, content] of files) {
        const hash = metadata.fileHashSnapshot[filePath] ?? '';
        insertFile.run({
          checkpointId: id,
          path: filePath,
          data: content,
          hash,
          isDeleted: 0,
        });
      }

      // Record deleted files in incremental checkpoints
      if (metadata.changes) {
        for (const change of metadata.changes) {
          if (change.type === 'deleted') {
            insertFile.run({
              checkpointId: id,
              path: change.path,
              data: null,
              hash: change.hash,
              isDeleted: 1,
            });
          }
        }
      }
    });

    transaction();
    logger.debug('File checkpoint saved', { checkpointId: id, fileCount: files.size });
  }

  async load(id: string): Promise<{ metadata: FileCheckpointMetadata; files: Map<string, Buffer> } | null> {
    const row = this.db.prepare('SELECT * FROM file_cp_metadata WHERE id = ?').get(id) as RowData | undefined;
    if (!row) return null;

    const metadata = this.rowToMetadata(row);

    const fileRows = this.db.prepare(
      'SELECT path, data, is_deleted FROM file_cp_files WHERE checkpoint_id = ? AND is_deleted = 0',
    ).all(id) as Array<{ path: string; data: Buffer | null; is_deleted: number }>;

    const files = new Map<string, Buffer>();
    for (const fileRow of fileRows) {
      if (fileRow.data) {
        files.set(fileRow.path, fileRow.data);
      }
    }

    return { metadata, files };
  }

  async delete(id: string): Promise<void> {
    const transaction = this.db.transaction(() => {
      this.db.prepare('DELETE FROM file_cp_files WHERE checkpoint_id = ?').run(id);
      this.db.prepare('DELETE FROM file_cp_metadata WHERE id = ?').run(id);
    });
    transaction();
    logger.debug('File checkpoint deleted', { checkpointId: id });
  }

  async list(options?: FileCheckpointListOptions): Promise<string[]> {
    let query = 'SELECT id FROM file_cp_metadata WHERE 1=1';
    const params: unknown[] = [];

    if (options?.entityId) {
      query += ' AND entity_id = ?';
      params.push(options.entityId);
    }
    if (options?.type) {
      query += ' AND type = ?';
      params.push(options.type);
    }
    if (options?.timestampFrom) {
      query += ' AND timestamp >= ?';
      params.push(options.timestampFrom);
    }
    if (options?.timestampTo) {
      query += ' AND timestamp <= ?';
      params.push(options.timestampTo);
    }

    query += ' ORDER BY timestamp DESC';

    if (options?.limit) {
      query += ' LIMIT ?';
      params.push(options.limit);
    }
    if (options?.offset) {
      query += ' OFFSET ?';
      params.push(options.offset);
    }

    const rows = this.db.prepare(query).all(...params) as Array<{ id: string }>;
    return rows.map(r => r.id);
  }

  async listByEntity(
    entityId: string,
    options?: { limit?: number },
  ): Promise<Array<{ id: string; metadata: FileCheckpointMetadata }>> {
    let query = 'SELECT * FROM file_cp_metadata WHERE entity_id = ? ORDER BY timestamp DESC';
    const params: unknown[] = [entityId];

    if (options?.limit) {
      query += ' LIMIT ?';
      params.push(options.limit);
    }

    const rows = this.db.prepare(query).all(...params) as RowData[];
    return rows.map(r => ({
      id: r.id,
      metadata: this.rowToMetadata(r),
    }));
  }

  async getLatestByEntity(
    entityId: string,
  ): Promise<{ id: string; metadata: FileCheckpointMetadata; files?: Map<string, Buffer> } | null> {
    const rows = this.listByEntity(entityId, { limit: 1 });
    const list = await rows;
    if (list.length === 0) return null;
    return list[0] ?? null;
  }

  async deleteByEntity(entityId: string, keepLatest?: number): Promise<number> {
    const transaction = this.db.transaction(() => {
      if (keepLatest && keepLatest > 0) {
        // Get IDs to keep
        const keepRows = this.db.prepare(
          'SELECT id FROM file_cp_metadata WHERE entity_id = ? ORDER BY timestamp DESC LIMIT ?',
        ).all(entityId, keepLatest) as Array<{ id: string }>;

        const keepIds = new Set(keepRows.map(r => r.id));

        const allRows = this.db.prepare(
          'SELECT id FROM file_cp_metadata WHERE entity_id = ?',
        ).all(entityId) as Array<{ id: string }>;

        const deleteIds = allRows.filter(r => !keepIds.has(r.id)).map(r => r.id);

        for (const id of deleteIds) {
          this.db.prepare('DELETE FROM file_cp_files WHERE checkpoint_id = ?').run(id);
          this.db.prepare('DELETE FROM file_cp_metadata WHERE id = ?').run(id);
        }

        return deleteIds.length;
      } else {
        this.db.prepare('DELETE FROM file_cp_files WHERE checkpoint_id IN (SELECT id FROM file_cp_metadata WHERE entity_id = ?)').run(entityId);
        const result = this.db.prepare('DELETE FROM file_cp_metadata WHERE entity_id = ?').run(entityId);
        return result.changes;
      }
    });

    return transaction() as number;
  }

  /**
   * Create the database schema
   */
  private createSchema(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS file_cp_metadata (
        id TEXT PRIMARY KEY,
        entity_id TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        type TEXT NOT NULL CHECK(type IN ('full', 'incremental')),
        base_checkpoint_id TEXT,
        file_count INTEGER NOT NULL DEFAULT 0,
        empty_dirs TEXT DEFAULT '[]',
        total_size INTEGER NOT NULL DEFAULT 0,
        workspace_root TEXT NOT NULL,
        file_hash_snapshot TEXT NOT NULL,
        changes TEXT,
        created_at INTEGER NOT NULL
      )
    `);

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS file_cp_files (
        checkpoint_id TEXT NOT NULL,
        path TEXT NOT NULL,
        data BLOB,
        hash TEXT,
        is_deleted INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (checkpoint_id, path),
        FOREIGN KEY (checkpoint_id) REFERENCES file_cp_metadata(id) ON DELETE CASCADE
      )
    `);

    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_file_cp_entity_timestamp
      ON file_cp_metadata(entity_id, timestamp DESC)
    `);

    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_file_cp_base
      ON file_cp_metadata(base_checkpoint_id)
    `);

    // Enable foreign key constraint support
    this.db.pragma('foreign_keys = ON');
  }

  /**
   * Convert a database row to FileCheckpointMetadata
   */
  private rowToMetadata(row: RowData): FileCheckpointMetadata {
    return {
      entityId: row.entity_id,
      timestamp: row.timestamp,
      type: row.type,
      baseCheckpointId: row.base_checkpoint_id ?? undefined,
      fileCount: row.file_count,
      emptyDirs: JSON.parse(row.empty_dirs ?? '[]'),
      totalSize: row.total_size,
      workspaceRoot: row.workspace_root,
      fileHashSnapshot: JSON.parse(row.file_hash_snapshot),
      changes: row.changes ? JSON.parse(row.changes) : undefined,
    };
  }
}

/**
 * Internal row type for database query results
 */
interface RowData {
  id: string;
  entity_id: string;
  timestamp: number;
  type: 'full' | 'incremental';
  base_checkpoint_id: string | null;
  file_count: number;
  empty_dirs: string;
  total_size: number;
  workspace_root: string;
  file_hash_snapshot: string;
  changes: string | null;
  created_at: number;
}