/**
 * FileCheckpointManager
 *
 * Manages workspace file state checkpoints independently of VFS.
 * Uses a scan-and-compare approach: at checkpoint time, scans the workspace,
 * computes file hashes, compares with previous snapshot, and stores deltas.
 *
 * Design principles:
 * - Non-intrusive: no file I/O interception, no tool changes
 * - Snapshot-based: captures file state at checkpoint boundaries
 * - Incremental: hash comparison for delta detection
 * - Safe: non-fatal errors, file checkpoint failure does not block execution checkpoint
 */
import * as fs from 'node:fs';
import * as path from 'node:path';
import { createHash } from 'node:crypto';
import { getGlobalLogger } from '../logger/global-logger.js';
import type {
  FileCheckpointMetadata,
  FileChangeRecord,
} from '@wf-agent/types';
import type {
  FileCheckpointStorageAdapter,
  FileCheckpointCreateResult,
  FileCheckpointRestoreResult,
  FileCheckpointManagerConfig,
} from './file-checkpoint-types.js';

const logger = getGlobalLogger();

const DEFAULT_CONFIG: Partial<FileCheckpointManagerConfig> = {
  maxFileSize: 10 * 1024 * 1024,
  maxDeltaChainLength: 20,
  customIgnorePatterns: [],
};

const HARDCODED_IGNORE_PATTERNS = ['.git', 'node_modules'];

export class FileCheckpointManager {
  private storage: FileCheckpointStorageAdapter;
  private config: FileCheckpointManagerConfig;
  private initialized = false;

  constructor(
    storage: FileCheckpointStorageAdapter,
    config: FileCheckpointManagerConfig,
  ) {
    this.storage = storage;
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;
    await this.storage.initialize();
    this.initialized = true;
    logger.info('FileCheckpointManager initialized', { workspaceRoot: this.config.workspaceRoot });
  }

  async close(): Promise<void> {
    if (!this.initialized) return;
    await this.storage.close();
    this.initialized = false;
    logger.info('FileCheckpointManager closed');
  }

  async createCheckpoint(entityId: string): Promise<FileCheckpointCreateResult> {
    if (!this.initialized) {
      throw new Error('FileCheckpointManager not initialized');
    }

    const { files, dirs } = await this.collectFiles(this.config.workspaceRoot);
    const fileHashes = await this.computeFileHashes(files);
    const emptyDirs = this.findEmptyDirs(dirs, files);

    const previous = await this.storage.getLatestByEntity(entityId);
    const prevHashSnapshot = previous?.metadata.fileHashSnapshot ?? {};

    const deltaChainLength = await this.getDeltaChainLength(entityId);
    const isFullBackup = !previous || deltaChainLength >= (this.config.maxDeltaChainLength ?? 20);

    const changes = this.computeChanges(prevHashSnapshot, fileHashes);

    const checkpointId = this.generateCheckpointId();
    const timestamp = Date.now();

    const metadata: FileCheckpointMetadata = {
      entityId,
      timestamp,
      type: isFullBackup ? 'full' : 'incremental',
      baseCheckpointId: isFullBackup ? undefined : previous!.id,
      changes: isFullBackup ? undefined : changes,
      fileCount: files.length,
      fileHashSnapshot: fileHashes,
      emptyDirs,
      totalSize: 0,
      workspaceRoot: this.config.workspaceRoot,
    };

    const fileContents = new Map<string, Buffer>();

    if (isFullBackup) {
      for (const filePath of files) {
        try {
          const content = await this.readFile(filePath);
          fileContents.set(filePath, content);
        } catch (err) {
          logger.warn('Failed to read file for full backup, skipping', { err, filePath });
        }
      }
    } else {
      for (const change of changes ?? []) {
        if (change.type === 'added' || change.type === 'modified') {
          try {
            const content = await this.readFile(change.path);
            fileContents.set(change.path, content);
          } catch (err) {
            logger.warn('Failed to read changed file, skipping', { err, path: change.path });
          }
        }
      }
    }

    let totalSize = 0;
    for (const [, content] of fileContents) {
      totalSize += content.length;
    }
    metadata.totalSize = totalSize;

    await this.storage.save(checkpointId, metadata, fileContents);

    logger.info('File checkpoint created', {
      checkpointId,
      entityId,
      type: metadata.type,
      fileCount: metadata.fileCount,
      changedFiles: changes?.length ?? files.length,
      totalSize,
    });

    return { id: checkpointId, metadata };
  }

  async restoreCheckpoint(entityId: string, checkpointId: string): Promise<FileCheckpointRestoreResult> {
    if (!this.initialized) {
      throw new Error('FileCheckpointManager not initialized');
    }

    const checkpoint = await this.storage.load(checkpointId);
    if (!checkpoint) {
      throw new Error(`File checkpoint not found: ${checkpointId}`);
    }

    const targetFiles = await this.resolveTargetFiles(checkpointId);

    const currentFiles = await this.collectFiles(this.config.workspaceRoot);
    const currentRelativeSet = new Set(currentFiles.files);

    const filesToDelete: string[] = [];
    const filesToRestore: Array<{ relativePath: string; content: Buffer }> = [];
    let skippedCount = 0;

    for (const [relativePath, content] of targetFiles) {
      const absolutePath = path.resolve(this.config.workspaceRoot, relativePath);

      if (!fs.existsSync(absolutePath)) {
        filesToRestore.push({ relativePath, content });
      } else {
        const currentHash = await this.hashFile(absolutePath);
        const targetHash = this.hashContent(content);

        if (currentHash === targetHash) {
          skippedCount++;
        } else {
          filesToRestore.push({ relativePath, content });
        }
      }
    }

    for (const relativePath of currentRelativeSet) {
      if (!targetFiles.has(relativePath)) {
        if (!this.shouldIgnore(relativePath)) {
          filesToDelete.push(relativePath);
        }
      }
    }

    for (const relativePath of filesToDelete) {
      try {
        const absolutePath = path.resolve(this.config.workspaceRoot, relativePath);
        await fs.promises.unlink(absolutePath);
        logger.debug('Deleted extra file during restore', { path: relativePath });
      } catch (err) {
        logger.warn('Failed to delete extra file', { err, path: relativePath });
      }
    }

    for (const { relativePath, content } of filesToRestore) {
      try {
        const absolutePath = path.resolve(this.config.workspaceRoot, relativePath);
        await fs.promises.mkdir(path.dirname(absolutePath), { recursive: true });
        await fs.promises.writeFile(absolutePath, content);
        logger.debug('Restored file', { path: relativePath });
      } catch (err) {
        logger.warn('Failed to restore file', { err, path: relativePath });
      }
    }

    for (const emptyDir of checkpoint.metadata.emptyDirs) {
      try {
        const absolutePath = path.resolve(this.config.workspaceRoot, emptyDir);
        await fs.promises.mkdir(absolutePath, { recursive: true });
      } catch (err) {
        logger.warn('Failed to restore empty directory', { err, emptyDir });
      }
    }

    logger.info('File checkpoint restored', {
      checkpointId,
      entityId,
      restoredCount: filesToRestore.length,
      deletedCount: filesToDelete.length,
      skippedCount,
    });

    return {
      restoredCount: filesToRestore.length,
      deletedCount: filesToDelete.length,
      skippedCount,
    };
  }

  getStorage(): FileCheckpointStorageAdapter {
    return this.storage;
  }

  private async collectFiles(rootDir: string): Promise<{ files: string[]; dirs: string[] }> {
    const files: string[] = [];
    const allDirs: string[] = [];

    const gitignorePatterns = await this.loadGitignorePatterns(rootDir);

    const allPatterns = [
      ...HARDCODED_IGNORE_PATTERNS,
      ...gitignorePatterns,
      ...(this.config.customIgnorePatterns ?? []),
    ];

    await this.scanDirectory(rootDir, rootDir, allPatterns, files, allDirs);

    return { files, dirs: allDirs };
  }

  private async scanDirectory(
    rootDir: string,
    currentDir: string,
    ignorePatterns: string[],
    files: string[],
    allDirs: string[],
  ): Promise<void> {
    let entries: fs.Dirent[];
    try {
      entries = await fs.promises.readdir(currentDir, { withFileTypes: true });
    } catch {
      return;
    }

    for (const entry of entries) {
      const relativePath = path.relative(rootDir, path.join(currentDir, entry.name));

      if (this.isIgnored(relativePath, entry.name, ignorePatterns)) {
        continue;
      }

      if (entry.isDirectory()) {
        allDirs.push(relativePath);
        await this.scanDirectory(rootDir, path.resolve(currentDir, entry.name), ignorePatterns, files, allDirs);
      } else if (entry.isFile()) {
        files.push(relativePath);
      }
    }
  }

  private async loadGitignorePatterns(rootDir: string): Promise<string[]> {
    const patterns: string[] = [];
    await this.collectGitignoreFiles(rootDir, rootDir, patterns);
    return patterns;
  }

  private async collectGitignoreFiles(rootDir: string, currentDir: string, patterns: string[]): Promise<void> {
    const gitignorePath = path.join(currentDir, '.gitignore');

    try {
      const content = await fs.promises.readFile(gitignorePath, 'utf-8');
      const relativePrefix = path.relative(rootDir, currentDir);

      for (const line of content.split('\n')) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('#')) continue;

        const prefixed = relativePrefix === ''
          ? trimmed
          : path.posix.join(relativePrefix.replace(/\\/g, '/'), trimmed);

        patterns.push(prefixed);
      }
    } catch {
      // File not found or permission error, skip
    }

    let entries: fs.Dirent[];
    try {
      entries = await fs.promises.readdir(currentDir, { withFileTypes: true });
    } catch {
      return;
    }

    for (const entry of entries) {
      if (entry.isDirectory() && entry.name !== '.git' && entry.name !== 'node_modules') {
        await this.collectGitignoreFiles(rootDir, path.join(currentDir, entry.name), patterns);
      }
    }
  }

  private isIgnored(relativePath: string, entryName: string, patterns: string[]): boolean {
    const normalizedPath = relativePath.replace(/\\/g, '/');

    for (const pattern of patterns) {
      const normalizedPattern = pattern.replace(/\\/g, '/');

      if (normalizedPattern.endsWith('/')) {
        const prefix = normalizedPattern.slice(0, -1);
        if (normalizedPath.startsWith(prefix) || normalizedPath === prefix) {
          return true;
        }
      }

      if (normalizedPath === normalizedPattern || normalizedPath.startsWith(normalizedPattern + '/')) {
        return true;
      }

      if (entryName === normalizedPattern) {
        return true;
      }

      if (normalizedPattern.includes('*')) {
        const regex = new RegExp(
          '^' + normalizedPattern.replace(/\*/g, '[^/]*').replace(/\?/g, '[^/]') + '$',
        );
        if (regex.test(normalizedPath) || regex.test(entryName)) {
          return true;
        }
      }
    }

    return false;
  }

  private shouldIgnore(relativePath: string): boolean {
    return this.isIgnored(
      relativePath,
      path.basename(relativePath),
      HARDCODED_IGNORE_PATTERNS,
    );
  }

  private async computeFileHashes(files: string[]): Promise<Record<string, string>> {
    const result: Record<string, string> = {};

    for (const filePath of files) {
      try {
        const absolutePath = path.resolve(this.config.workspaceRoot, filePath);
        result[filePath] = await this.hashFile(absolutePath);
      } catch (err) {
        logger.warn('Failed to hash file, skipping', { err, filePath });
      }
    }

    return result;
  }

  private async hashFile(absolutePath: string): Promise<string> {
    const content = await fs.promises.readFile(absolutePath);
    return this.hashContent(content);
  }

  private hashContent(content: Buffer): string {
    return createHash('md5').update(content).digest('hex');
  }

  private computeChanges(
    oldHashes: Record<string, string>,
    newHashes: Record<string, string>,
  ): FileChangeRecord[] {
    const changes: FileChangeRecord[] = [];

    for (const [filePath, newHash] of Object.entries(newHashes)) {
      const oldHash = oldHashes[filePath];

      if (!oldHash) {
        changes.push({ path: filePath, type: 'added', hash: newHash });
      } else if (oldHash !== newHash) {
        changes.push({ path: filePath, type: 'modified', hash: newHash });
      }
    }

    for (const [filePath, oldHash] of Object.entries(oldHashes)) {
      if (!(filePath in newHashes)) {
        changes.push({ path: filePath, type: 'deleted', hash: oldHash });
      }
    }

    return changes;
  }

  private findEmptyDirs(allDirs: string[], files: string[]): string[] {
    const fileParentDirs = new Set<string>();

    for (const filePath of files) {
      let dir = path.dirname(filePath);
      while (dir !== '.') {
        fileParentDirs.add(dir);
        dir = path.dirname(dir);
      }
    }

    return allDirs.filter(dir => !fileParentDirs.has(dir));
  }

  private async getDeltaChainLength(entityId: string): Promise<number> {
    const checkpoints = await this.storage.listByEntity(entityId);
    if (checkpoints.length === 0) return 0;

    let length = 0;
    for (const cp of checkpoints) {
      if (cp.metadata.type === 'incremental') {
        length++;
      }
    }

    return length;
  }

  private async resolveTargetFiles(
    checkpointId: string,
  ): Promise<Map<string, Buffer>> {
    const targetFiles = new Map<string, Buffer>();
    const visited = new Set<string>();

    const chain: Array<{ id: string; metadata: FileCheckpointMetadata }> = [];
    let currentId: string | undefined = checkpointId;

    while (currentId && !visited.has(currentId)) {
      visited.add(currentId);
      const checkpoint = await this.storage.load(currentId);
      if (!checkpoint) break;

      chain.unshift({ id: currentId, metadata: checkpoint.metadata });
      currentId = checkpoint.metadata.baseCheckpointId;
    }

    for (const link of chain) {
      const checkpoint = await this.storage.load(link.id);
      if (!checkpoint) continue;

      if (link.metadata.type === 'full') {
        for (const [relativePath, content] of checkpoint.files) {
          targetFiles.set(relativePath, content);
        }
      } else {
        for (const change of link.metadata.changes ?? []) {
          if (change.type === 'deleted') {
            targetFiles.delete(change.path);
          } else {
            const content = checkpoint.files.get(change.path);
            if (content) {
              targetFiles.set(change.path, content);
            }
          }
        }
      }
    }

    return targetFiles;
  }

  private async readFile(relativePath: string): Promise<Buffer> {
    const absolutePath = path.resolve(this.config.workspaceRoot, relativePath);
    return fs.promises.readFile(absolutePath);
  }

  private generateCheckpointId(): string {
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substring(2, 10);
    return `${timestamp}-${random}`;
  }
}