/**
 * DeltaStore — SQLite-backed file state store for checkpoint/history recording
 *
 * DeltaStore is a standalone storage layer for recording file changes.
 * It is NOT part of the VFS data path — SandboxVFS no longer uses delta
 * internally. Instead, DeltaStore serves as a recording layer for
 * checkpoint integration: it can snapshot file state before/after operations
 * and restore on rollback.
 *
 * This separation ensures that VFS (policy enforcement) and checkpoint
 * (history recording) are orthogonal concerns that can evolve independently.
 *
 * Schema:
 *   vfs_files:  path TEXT PRIMARY KEY, data BLOB, mode INT, created_at INT, modified_at INT
 *   vfs_dirs:   path TEXT PRIMARY KEY, mode INT, created_at INT, modified_at INT
 *   vfs_symlinks: path TEXT PRIMARY KEY, target TEXT NOT NULL, created_at INT
 */

import Database from "better-sqlite3";
import { basename } from "node:path";
import { existsSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import crypto from "node:crypto";
import type { VFSEntry, DeltaFileSystem, Snapshotable } from "../types.js";

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

export interface DeltaStoreConfig {
  dbPath: string;
  autoCreateDir?: boolean;
}

export class DeltaStore implements DeltaFileSystem, Snapshotable {
  private db: Database.Database;

  constructor(config: DeltaStoreConfig) {
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

      CREATE TABLE IF NOT EXISTS vfs_symlinks (
        path TEXT PRIMARY KEY,
        target TEXT NOT NULL,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS vfs_snapshots (
        id TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS vfs_snapshot_files (
        snapshot_id TEXT NOT NULL,
        path TEXT NOT NULL,
        data BLOB NOT NULL,
        mode INTEGER NOT NULL DEFAULT 644,
        size INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        modified_at INTEGER NOT NULL,
        PRIMARY KEY (snapshot_id, path),
        FOREIGN KEY (snapshot_id) REFERENCES vfs_snapshots(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS vfs_snapshot_dirs (
        snapshot_id TEXT NOT NULL,
        path TEXT NOT NULL,
        mode INTEGER NOT NULL DEFAULT 755,
        created_at INTEGER NOT NULL,
        modified_at INTEGER NOT NULL,
        PRIMARY KEY (snapshot_id, path),
        FOREIGN KEY (snapshot_id) REFERENCES vfs_snapshots(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS vfs_snapshot_symlinks (
        snapshot_id TEXT NOT NULL,
        path TEXT NOT NULL,
        target TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (snapshot_id, path),
        FOREIGN KEY (snapshot_id) REFERENCES vfs_snapshots(id) ON DELETE CASCADE
      );
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

    const symlink = this.db
      .prepare("SELECT path, target, created_at FROM vfs_symlinks WHERE path = ?")
      .get(normalized) as { path: string; target: string; created_at: number } | undefined;

    if (symlink) {
      return {
        name: basename(symlink.path),
        path: symlink.path,
        type: "file",
        size: 0,
        mode: 0o777,
        createdAt: symlink.created_at,
        modifiedAt: symlink.created_at,
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
      this.db.prepare("DELETE FROM vfs_symlinks WHERE path = ?").run(normalized);

      const prefix = `${normalized}/`;
      this.db.prepare("DELETE FROM vfs_files WHERE path LIKE ?").run(`${prefix}%`);
      this.db.prepare("DELETE FROM vfs_symlinks WHERE path LIKE ?").run(`${prefix}%`);

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
  // Symbolic link operations
  // =========================================================================

  async symlink(target: string, path: string): Promise<void> {
    const normalized = this.normalize(path);
    const now = Date.now();

    this.db
      .prepare(
        `INSERT INTO vfs_symlinks (path, target, created_at)
         VALUES (?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET target = excluded.target, created_at = excluded.created_at`,
      )
      .run(normalized, target, now);

    this.ensureParentDirSync(normalized);
  }

  async readlink(path: string): Promise<string | null> {
    const normalized = this.normalize(path);

    const row = this.db
      .prepare("SELECT target FROM vfs_symlinks WHERE path = ?")
      .get(normalized) as { target: string } | undefined;

    return row?.target ?? null;
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

    const symlinks = this.db
      .prepare(
        `SELECT path, target, created_at FROM vfs_symlinks
         WHERE path LIKE ? AND path != ?
         AND instr(substr(path, ?), '/') = 0`,
      )
      .all(`${prefix}%`, normalized, prefix.length + 1) as {
      path: string;
      target: string;
      created_at: number;
    }[];

    for (const symlink of symlinks) {
      entries.push({
        name: basename(symlink.path),
        path: symlink.path,
        type: "file",
        size: 0,
        mode: 0o777,
        createdAt: symlink.created_at,
        modifiedAt: symlink.created_at,
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

  close(): void {
    this.db.close();
  }

  // =========================================================================
  // Snapshot API (Snapshotable)
  // =========================================================================

  async snapshot(): Promise<string> {
    const snapshotId = crypto.randomUUID();
    const now = Date.now();

    this.db.transaction(() => {
      // Register snapshot
      this.db
        .prepare("INSERT INTO vfs_snapshots (id, created_at) VALUES (?, ?)")
        .run(snapshotId, now);

      // Snapshot all files
      const files = this.db
        .prepare("SELECT path, data, mode, size, created_at, modified_at FROM vfs_files")
        .all() as FileRow[];

      const insertFile = this.db.prepare(
        `INSERT INTO vfs_snapshot_files (snapshot_id, path, data, mode, size, created_at, modified_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      );
      for (const file of files) {
        insertFile.run(
          snapshotId,
          file.path,
          file.data,
          file.mode,
          file.size,
          file.created_at,
          file.modified_at,
        );
      }

      // Snapshot all dirs
      const dirs = this.db
        .prepare("SELECT path, mode, created_at, modified_at FROM vfs_dirs")
        .all() as DirRow[];

      const insertDir = this.db.prepare(
        `INSERT INTO vfs_snapshot_dirs (snapshot_id, path, mode, created_at, modified_at)
         VALUES (?, ?, ?, ?, ?)`,
      );
      for (const dir of dirs) {
        insertDir.run(snapshotId, dir.path, dir.mode, dir.created_at, dir.modified_at);
      }

      // Snapshot all symlinks
      const symlinks = this.db
        .prepare("SELECT path, target, created_at FROM vfs_symlinks")
        .all() as { path: string; target: string; created_at: number }[];

      const insertSymlink = this.db.prepare(
        `INSERT INTO vfs_snapshot_symlinks (snapshot_id, path, target, created_at)
         VALUES (?, ?, ?, ?)`,
      );
      for (const symlink of symlinks) {
        insertSymlink.run(snapshotId, symlink.path, symlink.target, symlink.created_at);
      }
    })();

    return snapshotId;
  }

  async restore(snapshotId: string): Promise<void> {
    const snapshot = this.db
      .prepare("SELECT id FROM vfs_snapshots WHERE id = ?")
      .get(snapshotId) as { id: string } | undefined;

    if (!snapshot) {
      throw new Error(`Snapshot not found: ${snapshotId}`);
    }

    this.db.transaction(() => {
      // Clear current data
      this.db.exec("DELETE FROM vfs_files");
      this.db.exec("DELETE FROM vfs_dirs");
      this.db.exec("DELETE FROM vfs_symlinks");

      // Restore files from snapshot
      const files = this.db
        .prepare(
          "SELECT path, data, mode, size, created_at, modified_at FROM vfs_snapshot_files WHERE snapshot_id = ?",
        )
        .all(snapshotId) as FileRow[];

      const insertFile = this.db.prepare(
        `INSERT INTO vfs_files (path, data, mode, size, created_at, modified_at)
         VALUES (?, ?, ?, ?, ?, ?)`,
      );
      for (const file of files) {
        insertFile.run(file.path, file.data, file.mode, file.size, file.created_at, file.modified_at);
      }

      // Restore dirs from snapshot
      const dirs = this.db
        .prepare("SELECT path, mode, created_at, modified_at FROM vfs_snapshot_dirs WHERE snapshot_id = ?")
        .all(snapshotId) as DirRow[];

      const insertDir = this.db.prepare(
        `INSERT INTO vfs_dirs (path, mode, created_at, modified_at)
         VALUES (?, ?, ?, ?)`,
      );
      for (const dir of dirs) {
        insertDir.run(dir.path, dir.mode, dir.created_at, dir.modified_at);
      }

      // Restore symlinks from snapshot
      const symlinks = this.db
        .prepare("SELECT path, target, created_at FROM vfs_snapshot_symlinks WHERE snapshot_id = ?")
        .all(snapshotId) as { path: string; target: string; created_at: number }[];

      const insertSymlink = this.db.prepare(
        `INSERT INTO vfs_symlinks (path, target, created_at)
         VALUES (?, ?, ?)`,
      );
      for (const symlink of symlinks) {
        insertSymlink.run(symlink.path, symlink.target, symlink.created_at);
      }
    })();
  }

  async listSnapshots(): Promise<Array<{ id: string; createdAt: number }>> {
    const rows = this.db
      .prepare("SELECT id, created_at FROM vfs_snapshots ORDER BY created_at DESC")
      .all() as { id: string; created_at: number }[];

    return rows.map((row) => ({ id: row.id, createdAt: row.created_at }));
  }

  async deleteSnapshot(snapshotId: string): Promise<void> {
    const result = this.db.prepare("DELETE FROM vfs_snapshots WHERE id = ?").run(snapshotId);
    if (result.changes === 0) {
      throw new Error(`Snapshot not found: ${snapshotId}`);
    }
  }

  // =========================================================================
  // Internal helpers
  // =========================================================================

  private normalize(p: string): string {
    let normalized = p.replace(/\\/g, "/").replace(/\/+/g, "/");
    if (normalized.length > 1 && normalized.endsWith("/")) {
      normalized = normalized.slice(0, -1);
    }
    return normalized.startsWith("/") ? normalized : "/" + normalized;
  }

  /**
   * Synchronously ensure the parent directory of a VFS path exists.
   * This is called after write operations to maintain directory hierarchy.
   */
  private ensureParentDirSync(p: string): void {
    const parent = dirname(p);
    if (parent === "/" || parent === ".") return;

    const now = Date.now();
    this.db
      .prepare(
        `INSERT OR IGNORE INTO vfs_dirs (path, mode, created_at, modified_at)
         VALUES (?, 755, ?, ?)`,
      )
      .run(parent, now, now);
  }
}