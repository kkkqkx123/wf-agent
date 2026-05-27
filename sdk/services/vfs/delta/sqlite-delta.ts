/**
 * SqliteDelta — SQLite-backed writable filesystem layer for sandbox VFS
 *
 * Replaces MemoryDelta with persistent storage:
 * - All file writes are persisted in SQLite immediately
 * - Snapshot/restore for checkpoint integration via snapshot table
 * - No in-memory state accumulation
 *
 * Schema:
 *   vfs_files:  path TEXT PRIMARY KEY, data BLOB, mode INT, created_at INT, modified_at INT
 *   vfs_dirs:   path TEXT PRIMARY KEY, mode INT, created_at INT, modified_at INT
 *   vfs_snapshots: id TEXT PRIMARY KEY, created_at INT
 *   vfs_snapshot_files: snapshot_id TEXT, path TEXT, data BLOB, mode INT, modified_at INT
 *                      PRIMARY KEY(snapshot_id, path)
 */

import Database from "better-sqlite3";
import { randomUUID } from "node:crypto";
import { basename } from "node:path";
import { existsSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import type { VFSEntry, DeltaFileSystem } from "../types.js";

interface FileRow {
  path: string;
  data: Buffer;
  mode: number;
  size: number;
  created_at: number;
  modified_at: number;
}

interface DirRow {
  path: string;
  mode: number;
  created_at: number;
  modified_at: number;
}

interface SnapshotFileRow {
  snapshot_id: string;
  path: string;
  data: Buffer;
  mode: number;
  modified_at: number;
}

export interface SqliteDeltaConfig {
  dbPath: string;
  autoCreateDir?: boolean;
}

export class SqliteDelta implements DeltaFileSystem {
  private db: Database.Database;

  constructor(config: SqliteDeltaConfig) {
    if (config.autoCreateDir !== false) {
      const dbDir = dirname(config.dbPath);
      if (!existsSync(dbDir)) {
        mkdirSync(dbDir, { recursive: true });
      }
    }

    this.db = new Database(config.dbPath);

    this.db.pragma("journal_mode = WAL");
    this.db.pragma("synchronous = NORMAL");

    this.initializeSchema();
  }

  private initializeSchema(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS vfs_files (
        path TEXT PRIMARY KEY,
        data BLOB NOT NULL,
        mode INTEGER NOT NULL DEFAULT 644,
        size INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        modified_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS vfs_dirs (
        path TEXT PRIMARY KEY,
        mode INTEGER NOT NULL DEFAULT 755,
        created_at INTEGER NOT NULL,
        modified_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS vfs_snapshots (
        id TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS vfs_snapshot_files (
        snapshot_id TEXT NOT NULL,
        path TEXT NOT NULL,
        data BLOB,
        mode INTEGER NOT NULL DEFAULT 644,
        modified_at INTEGER NOT NULL,
        is_deleted INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (snapshot_id, path)
      );

      CREATE INDEX IF NOT EXISTS idx_snapshot_files_id ON vfs_snapshot_files(snapshot_id);
    `);
  }

  // =========================================================================
  // File operations
  // =========================================================================

  async stat(path: string): Promise<VFSEntry | null> {
    const normalized = this.normalize(path);

    const file = this.db
      .prepare("SELECT path, mode, size, created_at, modified_at FROM vfs_files WHERE path = ?")
      .get(normalized) as FileRow | undefined;

    if (file) {
      return {
        name: basename(file.path),
        path: file.path,
        type: "file",
        size: file.size,
        mode: file.mode,
        createdAt: file.created_at,
        modifiedAt: file.modified_at,
      };
    }

    const dir = this.db
      .prepare("SELECT path, mode, created_at, modified_at FROM vfs_dirs WHERE path = ?")
      .get(normalized) as DirRow | undefined;

    if (dir) {
      return {
        name: basename(dir.path === "/" ? "/" : dir.path),
        path: dir.path,
        type: "directory",
        size: 0,
        mode: dir.mode,
        createdAt: dir.created_at,
        modifiedAt: dir.modified_at,
      };
    }

    return null;
  }

  async readFile(path: string): Promise<Buffer | null> {
    const normalized = this.normalize(path);

    const row = this.db.prepare("SELECT data FROM vfs_files WHERE path = ?").get(normalized) as
      | { data: Buffer }
      | undefined;

    return row?.data ?? null;
  }

  async writeFile(path: string, data: Buffer): Promise<void> {
    const normalized = this.normalize(path);
    const now = Date.now();

    this.db
      .prepare(
        `INSERT INTO vfs_files (path, data, mode, size, created_at, modified_at)
         VALUES (?, ?, 644, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET
           data = excluded.data,
           size = excluded.size,
           modified_at = excluded.modified_at`,
      )
      .run(normalized, data, data.length, now, now);

    this.ensureParentDirSync(normalized);
  }

  async remove(path: string): Promise<void> {
    const normalized = this.normalize(path);

    this.db.transaction(() => {
      this.db.prepare("DELETE FROM vfs_files WHERE path = ?").run(normalized);

      const prefix = `${normalized}/`;
      this.db.prepare("DELETE FROM vfs_files WHERE path LIKE ?").run(`${prefix}%`);

      this.db.prepare("DELETE FROM vfs_dirs WHERE path = ?").run(normalized);
    })();
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    const oldNormalized = this.normalize(oldPath);
    const newNormalized = this.normalize(newPath);

    this.db.transaction(() => {
      const file = this.db
        .prepare("SELECT data, mode, created_at FROM vfs_files WHERE path = ?")
        .get(oldNormalized) as { data: Buffer; mode: number; created_at: number } | undefined;

      if (file) {
        const now = Date.now();
        this.db
          .prepare(
            `INSERT INTO vfs_files (path, data, mode, created_at, modified_at)
             VALUES (?, ?, ?, ?, ?)`,
          )
          .run(newNormalized, file.data, file.mode, file.created_at, now);

        this.db.prepare("DELETE FROM vfs_files WHERE path = ?").run(oldNormalized);
        this.ensureParentDirSync(newNormalized);
      }

      const dir = this.db
        .prepare("SELECT mode, created_at FROM vfs_dirs WHERE path = ?")
        .get(oldNormalized) as { mode: number; created_at: number } | undefined;

      if (dir) {
        const now = Date.now();
        this.db
          .prepare(
            `INSERT INTO vfs_dirs (path, mode, created_at, modified_at)
             VALUES (?, ?, ?, ?)`,
          )
          .run(newNormalized, dir.mode, dir.created_at, now);

        this.db.prepare("DELETE FROM vfs_dirs WHERE path = ?").run(oldNormalized);

        const prefix = `${oldNormalized}/`;
        const newPrefix = `${newNormalized}/`;
        const childFiles = this.db
          .prepare(
            "SELECT path, data, mode, created_at, modified_at FROM vfs_files WHERE path LIKE ?",
          )
          .all(`${prefix}%`) as FileRow[];

        for (const child of childFiles) {
          const childNewPath = child.path.replace(prefix, newPrefix);
          this.db
            .prepare(
              `INSERT INTO vfs_files (path, data, mode, created_at, modified_at)
               VALUES (?, ?, ?, ?, ?)`,
            )
            .run(childNewPath, child.data, child.mode, child.created_at, child.modified_at);
          this.db.prepare("DELETE FROM vfs_files WHERE path = ?").run(child.path);
        }
      }
    })();
  }

  // =========================================================================
  // Directory operations
  // =========================================================================

  async readdir(path: string): Promise<VFSEntry[]> {
    const normalized = this.normalize(path);
    const prefix = normalized === "/" ? "" : `${normalized}/`;
    const entries: VFSEntry[] = [];

    const files = this.db
      .prepare(
        `SELECT path, mode, size, created_at, modified_at FROM vfs_files
         WHERE path LIKE ? AND path != ?
         AND instr(substr(path, ?), '/') = 0`,
      )
      .all(`${prefix}%`, normalized, prefix.length + 1) as FileRow[];

    for (const file of files) {
      entries.push({
        name: basename(file.path),
        path: file.path,
        type: "file",
        size: file.size,
        mode: file.mode,
        createdAt: file.created_at,
        modifiedAt: file.modified_at,
      });
    }

    const dirs = this.db
      .prepare(
        `SELECT path, mode, created_at, modified_at FROM vfs_dirs
         WHERE path LIKE ? AND path != ?
         AND instr(substr(path, ?), '/') = 0`,
      )
      .all(`${prefix}%`, normalized, prefix.length + 1) as DirRow[];

    for (const dir of dirs) {
      entries.push({
        name: basename(dir.path),
        path: dir.path,
        type: "directory",
        size: 0,
        mode: dir.mode,
        createdAt: dir.created_at,
        modifiedAt: dir.modified_at,
      });
    }

    return entries;
  }

  async mkdir(path: string): Promise<void> {
    const normalized = this.normalize(path);
    const now = Date.now();

    this.db
      .prepare(
        `INSERT INTO vfs_dirs (path, mode, created_at, modified_at)
         VALUES (?, 755, ?, ?)
         ON CONFLICT(path) DO NOTHING`,
      )
      .run(normalized, now, now);

    this.ensureParentDirSync(normalized);
  }

  async rmdir(path: string): Promise<void> {
    const normalized = this.normalize(path);

    this.db.transaction(() => {
      this.db.prepare("DELETE FROM vfs_dirs WHERE path = ?").run(normalized);

      const prefix = `${normalized}/`;
      this.db.prepare("DELETE FROM vfs_files WHERE path LIKE ?").run(`${prefix}%`);
    })();
  }

  // =========================================================================
  // Snapshot operations (for checkpoint integration)
  // =========================================================================

  createSnapshot(): string {
    const id = randomUUID();
    const now = Date.now();

    this.db.transaction(() => {
      this.db.prepare("INSERT INTO vfs_snapshots (id, created_at) VALUES (?, ?)").run(id, now);

      this.db
        .prepare(
          `INSERT INTO vfs_snapshot_files (snapshot_id, path, data, mode, modified_at, is_deleted)
           SELECT ?, path, data, mode, modified_at, 0 FROM vfs_files`,
        )
        .run(id);

      this.db
        .prepare(
          `INSERT INTO vfs_snapshot_files (snapshot_id, path, data, mode, modified_at, is_deleted)
           SELECT ?, path, NULL, 0, 0, 1 FROM vfs_dirs
           WHERE path NOT IN (SELECT path FROM vfs_snapshot_files WHERE snapshot_id = ?)`,
        )
        .run(id, id);
    })();

    return id;
  }

  restoreSnapshot(snapshotId: string): void {
    const snapshot = this.db
      .prepare("SELECT id FROM vfs_snapshots WHERE id = ?")
      .get(snapshotId) as { id: string } | undefined;

    if (!snapshot) {
      throw new Error(`VFS snapshot not found: ${snapshotId}`);
    }

    this.db.transaction(() => {
      const files = this.db
        .prepare(
          "SELECT path, data, mode, modified_at FROM vfs_snapshot_files WHERE snapshot_id = ? AND is_deleted = 0",
        )
        .all(snapshotId) as SnapshotFileRow[];

      this.db.prepare("DELETE FROM vfs_files").run();
      this.db.prepare("DELETE FROM vfs_dirs").run();

      const now = Date.now();
      const insertFile = this.db.prepare(
        `INSERT INTO vfs_files (path, data, mode, created_at, modified_at)
         VALUES (?, ?, ?, ?, ?)`,
      );

      const insertDir = this.db.prepare(
        `INSERT INTO vfs_dirs (path, mode, created_at, modified_at)
         VALUES (?, ?, ?, ?)`,
      );

      for (const file of files) {
        if (file.data) {
          insertFile.run(file.path, file.data, file.mode, now, file.modified_at);
        } else {
          insertDir.run(file.path, 755, now, file.modified_at);
        }
      }
    })();
  }

  deleteSnapshot(snapshotId: string): void {
    this.db.transaction(() => {
      this.db.prepare("DELETE FROM vfs_snapshot_files WHERE snapshot_id = ?").run(snapshotId);
      this.db.prepare("DELETE FROM vfs_snapshots WHERE id = ?").run(snapshotId);
    })();
  }

  listSnapshots(): string[] {
    const rows = this.db.prepare("SELECT id FROM vfs_snapshots ORDER BY created_at ASC").all() as {
      id: string;
    }[];
    return rows.map(r => r.id);
  }

  hasPendingChanges(): boolean {
    const fileCount = (
      this.db.prepare("SELECT COUNT(*) as count FROM vfs_files").get() as { count: number }
    ).count;
    const dirCount = (
      this.db.prepare("SELECT COUNT(*) as count FROM vfs_dirs").get() as { count: number }
    ).count;
    return fileCount > 0 || dirCount > 0;
  }

  close(): void {
    this.db.close();
  }

  // =========================================================================
  // Internal helpers
  // =========================================================================

  private normalize(path: string): string {
    const normalized = path.replace(/\\/g, "/").replace(/\/+/g, "/");

    if (normalized.length > 1 && normalized.endsWith("/")) {
      return normalized.slice(0, -1);
    }

    return normalized.startsWith("/") ? normalized : `/${normalized}`;
  }

  private ensureParentDirSync(vfsPath: string): void {
    const parentDir = vfsPath.substring(0, vfsPath.lastIndexOf("/"));
    if (!parentDir || parentDir === "") return;

    const now = Date.now();
    this.db
      .prepare(
        `INSERT INTO vfs_dirs (path, mode, created_at, modified_at)
         VALUES (?, 755, ?, ?)
         ON CONFLICT(path) DO NOTHING`,
      )
      .run(parentDir, now, now);

    const grandParent = parentDir.substring(0, parentDir.lastIndexOf("/"));
    if (grandParent && grandParent !== "") {
      this.ensureParentDirSync(grandParent);
    }
  }
}