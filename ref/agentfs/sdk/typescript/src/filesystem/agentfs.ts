import type { DatabasePromise } from '@tursodatabase/database-common';
import { createFsError, type FsSyscall } from '../errors.js';
import {
  assertInodeIsDirectory,
  assertNotRoot,
  assertNotSymlinkMode,
  assertReadableExistingInode,
  assertReaddirTargetInode,
  assertUnlinkTargetInode,
  assertWritableExistingInode,
  getInodeModeOrThrow,
  normalizeRmOptions,
  throwENOENTUnlessForce,
} from '../guards.js';
import {
  S_IFMT,
  S_IFREG,
  S_IFDIR,
  S_IFLNK,
  DEFAULT_FILE_MODE,
  DEFAULT_DIR_MODE,
  createStats,
  type Stats,
  type DirEntry,
  type FilesystemStats,
  type FileHandle,
  type FileSystem,
} from './interface.js';

/**
 * A simplified file handle for the simplified filesystem implementation.
 */
class SimplifiedFileHandle implements FileHandle {
  private db: DatabasePromise;
  private bufferCtor: BufferConstructor;
  private path: string;
  private content: Buffer;

  constructor(db: DatabasePromise, bufferCtor: BufferConstructor, path: string, content: Buffer) {
    this.db = db;
    this.bufferCtor = bufferCtor;
    this.path = path;
    this.content = content;
  }

  async pread(offset: number, size: number): Promise<Buffer> {
    const actualSize = Math.min(size, this.content.length - offset);
    if (actualSize <= 0) {
      return this.bufferCtor.alloc(0);
    }
    return this.content.subarray(offset, offset + actualSize);
  }

  async pwrite(offset: number, data: Buffer): Promise<void> {
    // Expand buffer if needed
    if (offset + data.length > this.content.length) {
      const newBuffer = this.bufferCtor.alloc(offset + data.length);
      this.content.copy(newBuffer);
      this.content = newBuffer;
    }

    data.copy(this.content, offset);

    // Update the file in the database
    const now = Math.floor(Date.now() / 1000);
    const updateStmt = this.db.prepare(`
      UPDATE files
      SET content = ?, size = ?, updated_at = ?
      WHERE path = ?
    `);
    await updateStmt.run(this.content, this.content.length, now, this.path);
  }

  async truncate(size: number): Promise<void> {
    if (size === 0) {
      this.content = this.bufferCtor.alloc(0);
    } else if (size < this.content.length) {
      this.content = this.content.subarray(0, size);
    } else if (size > this.content.length) {
      const newBuffer = this.bufferCtor.alloc(size);
      this.content.copy(newBuffer);
      this.content = newBuffer;
    }

    // Update the file in the database
    const now = Math.floor(Date.now() / 1000);
    const updateStmt = this.db.prepare(`
      UPDATE files
      SET content = ?, size = ?, updated_at = ?
      WHERE path = ?
    `);
    await updateStmt.run(this.content, this.content.length, now, this.path);
  }

  async fsync(): Promise<void> {
    // In SQLite, data is typically persisted automatically
    // This is a no-op in our simplified implementation
  }

  async fstat(): Promise<Stats> {
    // Get file stats from the database
    const stmt = this.db.prepare('SELECT size, permissions, created_at, updated_at FROM files WHERE path = ?');
    const row = await stmt.get(this.path) as {
      size: number;
      permissions: number;
      created_at: number;
      updated_at: number;
    } | undefined;

    if (!row) {
      throw new Error('File not found in database');
    }

    return createStats({
      ino: 0, // Simplified - not using inodes anymore
      mode: row.permissions,
      nlink: 1,
      uid: 0,
      gid: 0,
      size: row.size,
      atime: row.updated_at,
      mtime: row.updated_at,
      ctime: row.created_at,
    });
  }
}

/**
 * A simplified filesystem backed by SQLite, implementing the FileSystem interface.
 * This version uses a flat table structure instead of the complex inode/dentry/data system.
 */
export class AgentFS implements FileSystem {
  private db: DatabasePromise;
  private bufferCtor: BufferConstructor;

  private constructor(db: DatabasePromise, b: BufferConstructor) {
    this.db = db;
    this.bufferCtor = b;
  }

  static async fromDatabase(db: DatabasePromise, b?: BufferConstructor): Promise<AgentFS> {
    const fs = new AgentFS(db, b ?? Buffer);
    await fs.initialize();
    return fs;
  }

  private async initialize(): Promise<void> {
    // Create the simplified files table
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS files (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        path TEXT NOT NULL UNIQUE,
        parent_path TEXT NOT NULL,
        name TEXT NOT NULL,
        content BLOB,
        size INTEGER DEFAULT 0,
        mime_type TEXT,
        created_at INTEGER DEFAULT (unixepoch()),
        updated_at INTEGER DEFAULT (unixepoch()),
        permissions INTEGER DEFAULT 0o644,
        owner_id INTEGER DEFAULT 0,
        metadata JSON  -- For extended attributes
      )
    `);

    // Create indexes for efficient queries
    await this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_files_path ON files(path)
    `);
    await this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_files_parent_path ON files(parent_path)
    `);

    // Create root directory if it doesn't exist
    await this.ensureRoot();
  }

  private async ensureRoot(): Promise<void> {
    const stmt = this.db.prepare('SELECT path FROM files WHERE path = ?');
    const root = await stmt.get('/') as { path: string } | undefined;

    if (!root) {
      const now = Math.floor(Date.now() / 1000);
      const insertStmt = this.db.prepare(`
        INSERT INTO files (path, parent_path, name, content, size, permissions, updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `);
      await insertStmt.run('/', '/', '', null, 0, DEFAULT_DIR_MODE, now, now);
    }
  }

  private normalizePath(path: string): string {
    const normalized = path.replace(/\/+$/, '') || '/';
    return normalized.startsWith('/') ? normalized : '/' + normalized;
  }

  private async pathExists(path: string): Promise<boolean> {
    const normalizedPath = this.normalizePath(path);
    const stmt = this.db.prepare('SELECT 1 FROM files WHERE path = ? LIMIT 1');
    const result = await stmt.get(normalizedPath);
    return !!result;
  }

  private async getParentPath(path: string): Promise<string> {
    const normalized = this.normalizePath(path);
    if (normalized === '/') return '/';

    const parts = normalized.split('/').filter(p => p);
    if (parts.length === 1) return '/';

    return '/' + parts.slice(0, -1).join('/');
  }

  private async getFileName(path: string): Promise<string> {
    const normalized = this.normalizePath(path);
    if (normalized === '/') return '';

    const parts = normalized.split('/').filter(p => p);
    return parts[parts.length - 1];
  }

  private async ensureParentDirs(path: string): Promise<void> {
    const parentPath = await this.getParentPath(path);
    if (parentPath === '/') return; // Root already exists

    const parentExists = await this.pathExists(parentPath);
    if (!parentExists) {
      await this.ensureParentDirs(parentPath); // Recursively ensure parent exists
      await this.mkdir(parentPath);
    } else {
      // Verify parent is actually a directory
      const stmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
      const row = await stmt.get(parentPath) as { permissions: number } | undefined;
      if (row && (row.permissions & S_IFMT) !== S_IFDIR) {
        throw createFsError({
          code: 'ENOTDIR',
          syscall: 'mkdir',
          path: parentPath,
          message: 'not a directory',
        });
      }
    }
  }

  // ==================== FileSystem Interface Implementation ====================

  async writeFile(
    path: string,
    content: string | Buffer,
    options?: BufferEncoding | { encoding?: BufferEncoding }
  ): Promise<void> {
    const normalizedPath = this.normalizePath(path);
    const parentPath = await this.getParentPath(normalizedPath);
    const name = await this.getFileName(normalizedPath);

    // Ensure parent directories exist
    await this.ensureParentDirs(normalizedPath);

    // Convert content to buffer
    const encoding = typeof options === 'string'
      ? options
      : options?.encoding;

    const buffer = typeof content === 'string'
      ? this.bufferCtor.from(content, encoding ?? 'utf8')
      : content;

    const now = Math.floor(Date.now() / 1000);

    // Insert or update the file
    const stmt = this.db.prepare(`
      INSERT INTO files (path, parent_path, name, content, size, updated_at)
      VALUES (?, ?, ?, ?, ?, ?)
      ON CONFLICT(path) DO UPDATE SET
        content = excluded.content,
        size = excluded.size,
        updated_at = excluded.updated_at
    `);

    await stmt.run(normalizedPath, parentPath, name, buffer, buffer.length, now);
  }

  async readFile(path: string): Promise<Buffer>;
  async readFile(path: string, encoding: BufferEncoding): Promise<string>;
  async readFile(path: string, options: { encoding: BufferEncoding }): Promise<string>;
  async readFile(
    path: string,
    options?: BufferEncoding | { encoding?: BufferEncoding }
  ): Promise<Buffer | string> {
    const normalizedPath = this.normalizePath(path);

    const stmt = this.db.prepare('SELECT content, size FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as { content: Buffer | null; size: number } | undefined;

    if (!row) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'open',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if (row.content === null) {
      throw createFsError({
        code: 'EISDIR',
        syscall: 'open',
        path: normalizedPath,
        message: 'illegal operation on a directory',
      });
    }

    const encoding = typeof options === 'string'
      ? options
      : options?.encoding;

    // Update access time
    const now = Math.floor(Date.now() / 1000);
    const updateStmt = this.db.prepare('UPDATE files SET updated_at = ? WHERE path = ?');
    await updateStmt.run(now, normalizedPath);

    if (encoding) {
      return (row.content as Buffer).toString(encoding);
    }
    return row.content as Buffer;
  }

  async readdir(path: string): Promise<string[]> {
    const normalizedPath = this.normalizePath(path);

    // Check if path exists and is a directory
    const stmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as { permissions: number } | undefined;

    if (!row) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'scandir',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if ((row.permissions & S_IFMT) !== S_IFDIR) {
      throw createFsError({
        code: 'ENOTDIR',
        syscall: 'scandir',
        path: normalizedPath,
        message: 'not a directory',
      });
    }

    // Get all entries in this directory
    const entriesStmt = this.db.prepare(`
      SELECT name FROM files
      WHERE parent_path = ? AND path != ?
      ORDER BY name ASC
    `);
    const rows = await entriesStmt.all(normalizedPath, normalizedPath) as { name: string }[];

    return rows.map(row => row.name);
  }

  async readdirPlus(path: string): Promise<DirEntry[]> {
    const normalizedPath = this.normalizePath(path);

    // Check if path exists and is a directory
    const stmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as { permissions: number } | undefined;

    if (!row) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'scandir',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if ((row.permissions & S_IFMT) !== S_IFDIR) {
      throw createFsError({
        code: 'ENOTDIR',
        syscall: 'scandir',
        path: normalizedPath,
        message: 'not a directory',
      });
    }

    // Get all entries in this directory with stats
    const entriesStmt = this.db.prepare(`
      SELECT path, name, size, permissions, created_at, updated_at FROM files
      WHERE parent_path = ? AND path != ?
      ORDER BY name ASC
    `);
    const rows = await entriesStmt.all(normalizedPath, normalizedPath) as {
      path: string;
      name: string;
      size: number;
      permissions: number;
      created_at: number;
      updated_at: number;
    }[];

    return rows.map(row => ({
      name: row.name,
      stats: createStats({
        ino: 0, // Simplified - not using inodes anymore
        mode: row.permissions,
        nlink: 1,
        uid: 0,
        gid: 0,
        size: row.size,
        atime: row.updated_at,
        mtime: row.updated_at,
        ctime: row.created_at,
      }),
    }));
  }

  async stat(path: string): Promise<Stats> {
    const normalizedPath = this.normalizePath(path);

    const stmt = this.db.prepare('SELECT path, size, permissions, created_at, updated_at FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as {
      path: string;
      size: number;
      permissions: number;
      created_at: number;
      updated_at: number;
    } | undefined;

    if (!row) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'stat',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    return createStats({
      ino: 0, // Simplified - not using inodes anymore
      mode: row.permissions,
      nlink: 1,
      uid: 0,
      gid: 0,
      size: row.size,
      atime: row.updated_at,
      mtime: row.updated_at,
      ctime: row.created_at,
    });
  }

  async lstat(path: string): Promise<Stats> {
    // For the simplified implementation, lstat is the same as stat
    return this.stat(path);
  }

  async mkdir(path: string): Promise<void> {
    const normalizedPath = this.normalizePath(path);

    // Check if path already exists
    if (await this.pathExists(normalizedPath)) {
      throw createFsError({
        code: 'EEXIST',
        syscall: 'mkdir',
        path: normalizedPath,
        message: 'file already exists',
      });
    }

    const parentPath = await this.getParentPath(normalizedPath);
    const name = await this.getFileName(normalizedPath);

    // Ensure parent exists and is a directory
    const parentStmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
    const parentRow = await parentStmt.get(parentPath) as { permissions: number } | undefined;

    if (!parentRow) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'mkdir',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if ((parentRow.permissions & S_IFMT) !== S_IFDIR) {
      throw createFsError({
        code: 'ENOTDIR',
        syscall: 'mkdir',
        path: parentPath,
        message: 'not a directory',
      });
    }

    const now = Math.floor(Date.now() / 1000);

    // Create the directory
    const stmt = this.db.prepare(`
      INSERT INTO files (path, parent_path, name, content, size, permissions, updated_at, created_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `);

    await stmt.run(normalizedPath, parentPath, name, null, 0, DEFAULT_DIR_MODE, now, now);
  }

  async rmdir(path: string): Promise<void> {
    const normalizedPath = this.normalizePath(path);
    assertNotRoot(normalizedPath, 'rmdir');

    // Check if path exists and is a directory
    const stmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as { permissions: number } | undefined;

    if (!row) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'rmdir',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if ((row.permissions & S_IFMT) !== S_IFDIR) {
      throw createFsError({
        code: 'ENOTDIR',
        syscall: 'rmdir',
        path: normalizedPath,
        message: 'not a directory',
      });
    }

    // Check if directory is empty
    const childStmt = this.db.prepare('SELECT 1 FROM files WHERE parent_path = ? AND path != ? LIMIT 1');
    const child = await childStmt.get(normalizedPath, normalizedPath);

    if (child) {
      throw createFsError({
        code: 'ENOTEMPTY',
        syscall: 'rmdir',
        path: normalizedPath,
        message: 'directory not empty',
      });
    }

    // Remove the directory
    const deleteStmt = this.db.prepare('DELETE FROM files WHERE path = ?');
    await deleteStmt.run(normalizedPath);
  }

  async unlink(path: string): Promise<void> {
    const normalizedPath = this.normalizePath(path);
    assertNotRoot(normalizedPath, 'unlink');

    // Check if path exists and is a file
    const stmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as { permissions: number } | undefined;

    if (!row) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'unlink',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if ((row.permissions & S_IFMT) === S_IFDIR) {
      throw createFsError({
        code: 'EISDIR',
        syscall: 'unlink',
        path: normalizedPath,
        message: 'illegal operation on a directory',
      });
    }

    // Remove the file
    const deleteStmt = this.db.prepare('DELETE FROM files WHERE path = ?');
    await deleteStmt.run(normalizedPath);
  }

  async rm(
    path: string,
    options?: { force?: boolean; recursive?: boolean }
  ): Promise<void> {
    const normalizedPath = this.normalizePath(path);
    const { force, recursive } = normalizeRmOptions(options);
    assertNotRoot(normalizedPath, 'rm');

    // Check if path exists
    const stmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as { permissions: number } | undefined;

    if (!row) {
      if (force) return;
      throw createFsError({
        code: 'ENOENT',
        syscall: 'rm',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if ((row.permissions & S_IFMT) === S_IFDIR) {
      if (!recursive) {
        throw createFsError({
          code: 'EISDIR',
          syscall: 'rm',
          path: normalizedPath,
          message: 'illegal operation on a directory',
        });
      }

      // Recursively remove directory contents
      await this.rmDirContentsRecursive(normalizedPath);
    }

    // Remove the entry
    const deleteStmt = this.db.prepare('DELETE FROM files WHERE path = ?');
    await deleteStmt.run(normalizedPath);
  }

  private async rmDirContentsRecursive(dirPath: string): Promise<void> {
    // Get all entries in this directory
    const entriesStmt = this.db.prepare(`
      SELECT path, permissions FROM files
      WHERE parent_path = ? AND path != ?
    `);
    const rows = await entriesStmt.all(dirPath, dirPath) as { path: string; permissions: number }[];

    for (const row of rows) {
      if ((row.permissions & S_IFMT) === S_IFDIR) {
        await this.rmDirContentsRecursive(row.path);
      }

      // Remove the entry
      const deleteStmt = this.db.prepare('DELETE FROM files WHERE path = ?');
      await deleteStmt.run(row.path);
    }
  }

  async rename(oldPath: string, newPath: string): Promise<void> {
    const oldNormalized = this.normalizePath(oldPath);
    const newNormalized = this.normalizePath(newPath);

    if (oldNormalized === newNormalized) return;

    assertNotRoot(oldNormalized, 'rename');
    assertNotRoot(newNormalized, 'rename');

    // Check if old path exists
    const oldStmt = this.db.prepare('SELECT permissions, parent_path, name FROM files WHERE path = ?');
    const oldRow = await oldStmt.get(oldNormalized) as {
      permissions: number;
      parent_path: string;
      name: string
    } | undefined;

    if (!oldRow) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'rename',
        path: oldNormalized,
        message: 'no such file or directory',
      });
    }

    assertNotSymlinkMode(oldRow.permissions, 'rename', oldNormalized);
    const oldIsDir = (oldRow.permissions & S_IFMT) === S_IFDIR;

    if (oldIsDir && newNormalized.startsWith(oldNormalized + '/')) {
      throw createFsError({
        code: 'EINVAL',
        syscall: 'rename',
        path: newNormalized,
        message: 'invalid argument',
      });
    }

    // Check if new path exists
    const newStmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
    const newRow = await newStmt.get(newNormalized) as { permissions: number } | undefined;

    if (newRow) {
      assertNotSymlinkMode(newRow.permissions, 'rename', newNormalized);
      const newIsDir = (newRow.permissions & S_IFMT) === S_IFDIR;

      if (newIsDir && !oldIsDir) {
        throw createFsError({
          code: 'EISDIR',
          syscall: 'rename',
          path: newNormalized,
          message: 'illegal operation on a directory',
        });
      }
      if (!newIsDir && oldIsDir) {
        throw createFsError({
          code: 'ENOTDIR',
          syscall: 'rename',
          path: newNormalized,
          message: 'not a directory',
        });
      }

      if (newIsDir) {
        const childStmt = this.db.prepare('SELECT 1 FROM files WHERE parent_path = ? AND path != ? LIMIT 1');
        const child = await childStmt.get(newNormalized, newNormalized);
        if (child) {
          throw createFsError({
            code: 'ENOTEMPTY',
            syscall: 'rename',
            path: newNormalized,
            message: 'directory not empty',
          });
        }
      }

      // Remove the destination if it exists
      const deleteNewStmt = this.db.prepare('DELETE FROM files WHERE path = ?');
      await deleteNewStmt.run(newNormalized);
    }

    // Update the path and parent_path for the renamed item and all its children
    const newParentPath = await this.getParentPath(newNormalized);
    const newName = await this.getFileName(newNormalized);

    // Update the item itself
    const updateStmt = this.db.prepare(`
      UPDATE files
      SET path = ?, parent_path = ?, name = ?
      WHERE path = ?
    `);
    await updateStmt.run(newNormalized, newParentPath, newName, oldNormalized);

    // Update all children if it's a directory
    if (oldIsDir) {
      // Get all children
      const childrenStmt = this.db.prepare('SELECT path FROM files WHERE path LIKE ?');
      const children = await childrenStmt.all(oldNormalized + '/%') as { path: string }[];

      for (const child of children) {
        const relativePath = child.path.substring(oldNormalized.length);
        const newPath = newNormalized + relativePath;

        const updateChildStmt = this.db.prepare(`
          UPDATE files
          SET path = ?, parent_path = ?
          WHERE path = ?
        `);
        await updateChildStmt.run(newPath, await this.getParentPath(newPath), child.path);
      }
    }
  }

  async copyFile(src: string, dest: string): Promise<void> {
    const srcNormalized = this.normalizePath(src);
    const destNormalized = this.normalizePath(dest);

    if (srcNormalized === destNormalized) {
      throw createFsError({
        code: 'EINVAL',
        syscall: 'copyfile',
        path: destNormalized,
        message: 'invalid argument',
      });
    }

    // Check if source exists
    const srcStmt = this.db.prepare('SELECT permissions, content, size FROM files WHERE path = ?');
    const srcRow = await srcStmt.get(srcNormalized) as {
      permissions: number;
      content: Buffer | null;
      size: number
    } | undefined;

    if (!srcRow) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'copyfile',
        path: srcNormalized,
        message: 'no such file or directory',
      });
    }

    if ((srcRow.permissions & S_IFMT) === S_IFDIR) {
      throw createFsError({
        code: 'EISDIR',
        syscall: 'copyfile',
        path: srcNormalized,
        message: 'illegal operation on a directory',
      });
    }

    // Ensure destination parent exists
    await this.ensureParentDirs(destNormalized);

    // Copy the file
    const now = Math.floor(Date.now() / 1000);
    const destParentPath = await this.getParentPath(destNormalized);
    const destName = await this.getFileName(destNormalized);

    const insertStmt = this.db.prepare(`
      INSERT INTO files (path, parent_path, name, content, size, updated_at)
      VALUES (?, ?, ?, ?, ?, ?)
      ON CONFLICT(path) DO UPDATE SET
        content = excluded.content,
        size = excluded.size,
        updated_at = excluded.updated_at
    `);

    await insertStmt.run(
      destNormalized,
      destParentPath,
      destName,
      srcRow.content,
      srcRow.size,
      now
    );
  }

  async symlink(target: string, linkpath: string): Promise<void> {
    const normalizedLinkpath = this.normalizePath(linkpath);

    // Check if path already exists
    if (await this.pathExists(normalizedLinkpath)) {
      throw createFsError({
        code: 'EEXIST',
        syscall: 'open',
        path: normalizedLinkpath,
        message: 'file already exists',
      });
    }

    const parentPath = await this.getParentPath(normalizedLinkpath);
    const name = await this.getFileName(normalizedLinkpath);

    // Ensure parent exists and is a directory
    const parentStmt = this.db.prepare('SELECT permissions FROM files WHERE path = ?');
    const parentRow = await parentStmt.get(parentPath) as { permissions: number } | undefined;

    if (!parentRow) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'open',
        path: normalizedLinkpath,
        message: 'no such file or directory',
      });
    }

    if ((parentRow.permissions & S_IFMT) !== S_IFDIR) {
      throw createFsError({
        code: 'ENOTDIR',
        syscall: 'open',
        path: parentPath,
        message: 'not a directory',
      });
    }

    const now = Math.floor(Date.now() / 1000);

    // Create the symbolic link with special permissions
    const mode = S_IFLNK | 0o777;
    const insertStmt = this.db.prepare(`
      INSERT INTO files (path, parent_path, name, content, size, permissions, metadata, updated_at, created_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);

    await insertStmt.run(
      normalizedLinkpath,
      parentPath,
      name,
      Buffer.from(target, 'utf8'), // Store target in content
      target.length,
      mode,
      JSON.stringify({ target }), // Also store in metadata for easy access
      now,
      now
    );
  }

  async readlink(path: string): Promise<string> {
    const normalizedPath = this.normalizePath(path);

    const stmt = this.db.prepare('SELECT permissions, content, metadata FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as {
      permissions: number;
      content: Buffer | null;
      metadata: string
    } | undefined;

    if (!row) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'open',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if ((row.permissions & S_IFMT) !== S_IFLNK) {
      throw createFsError({
        code: 'EINVAL',
        syscall: 'open',
        path: normalizedPath,
        message: 'invalid argument',
      });
    }

    // Try to get target from metadata first, fallback to content
    if (row.metadata) {
      try {
        const meta = JSON.parse(row.metadata);
        if (meta.target) return meta.target;
      } catch (e) {
        // If metadata parsing fails, fall back to content
      }
    }

    if (row.content) {
      return (row.content as Buffer).toString('utf8');
    }

    throw createFsError({
      code: 'ENOENT',
      syscall: 'open',
      path: normalizedPath,
      message: 'no such file or directory',
    });
  }

  async access(path: string): Promise<void> {
    const normalizedPath = this.normalizePath(path);
    const exists = await this.pathExists(normalizedPath);

    if (!exists) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'access',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }
  }

  async statfs(): Promise<FilesystemStats> {
    const countStmt = this.db.prepare('SELECT COUNT(*) as count FROM files');
    const countRow = await countStmt.get() as { count: number };

    const sizeStmt = this.db.prepare('SELECT COALESCE(SUM(size), 0) as total FROM files WHERE content IS NOT NULL');
    const sizeRow = await sizeStmt.get() as { total: number };

    return {
      inodes: countRow.count,
      bytesUsed: sizeRow.total,
    };
  }

  async open(path: string): Promise<FileHandle> {
    // For the simplified implementation, we'll return a basic file handle that supports the required operations
    const normalizedPath = this.normalizePath(path);

    // Check if file exists and is readable
    const stmt = this.db.prepare('SELECT content, size FROM files WHERE path = ?');
    const row = await stmt.get(normalizedPath) as { content: Buffer | null; size: number } | undefined;

    if (!row) {
      throw createFsError({
        code: 'ENOENT',
        syscall: 'open',
        path: normalizedPath,
        message: 'no such file or directory',
      });
    }

    if (row.content === null) {
      throw createFsError({
        code: 'EISDIR',
        syscall: 'open',
        path: normalizedPath,
        message: 'illegal operation on a directory',
      });
    }

    // Update access time
    const now = Math.floor(Date.now() / 1000);
    const updateStmt = this.db.prepare('UPDATE files SET updated_at = ? WHERE path = ?');
    await updateStmt.run(now, normalizedPath);

    // Return a simplified file handle
    return new SimplifiedFileHandle(this.db, this.bufferCtor, normalizedPath, row.content);
  }

  // Legacy alias
  async deleteFile(path: string): Promise<void> {
    return await this.unlink(path);
  }
}
