/**
 * FileCheckpointManager (Optimized)
 *
 * Manages workspace file state checkpoints with incremental file watching.
 *
 * Optimization features:
 * - Chokidar-based file watching (optional) for O(K) instead of O(N) checkpoint
 * - Hash baseline storage for minimal initial storage
 * - Delta content storage for incremental checkpoints
 * - Myers diff algorithm for file comparison
 *
 * Backward compatible with existing CheckpointCoordinator integration.
 */

import * as fs from "fs";
import * as fsp from "fs/promises";
import * as path from "path";
import { createHash } from "crypto";
import { getGlobalLogger } from "../logger/global-logger.js";
import type { FileCheckpointMetadata, FileChangeRecord } from "@wf-agent/types";
import type {
  FileCheckpointStorageAdapter,
  FileCheckpointCreateResult,
  FileCheckpointRestoreResult,
  FileCheckpointManagerConfig,
} from "./file-checkpoint-types.js";
import { FileWatcher } from "./file-watcher.js";
import { HashBaselineStore, type HashBaseline } from "./hash-baseline-store.js";
import { FileDeltaStore } from "./file-delta-store.js";
import { DiffEngine, type DiffResult } from "./diff-engine.js";

const logger = getGlobalLogger();

const DEFAULT_CONFIG: Partial<FileCheckpointManagerConfig> = {
  maxFileSize: 10 * 1024 * 1024,
  maxDeltaChainLength: 20,
  customIgnorePatterns: [],
};

const HARDCODED_IGNORE_PATTERNS = [".git", "node_modules"];

/**
 * Optimized FileCheckpointManager configuration
 */
export interface OptimizedFileCheckpointManagerConfig extends FileCheckpointManagerConfig {
  /** Enable file watching for incremental change detection */
  useFileWatcher?: boolean;
  /** Enable hash baseline storage (reduces initial storage) */
  useHashBaseline?: boolean;
  /** Enable diff generation for changed files */
  enableDiff?: boolean;
}

/**
 * FileCheckpointManager with optional optimizations
 *
 * When optimizations are disabled, behaves identically to original implementation.
 * When enabled, uses FileWatcher + HashBaseline + FileDelta for efficiency.
 */
export class FileCheckpointManager {
  private storage: FileCheckpointStorageAdapter;
  private config: OptimizedFileCheckpointManagerConfig;
  private initialized = false;

  // Optimization components (optional)
  private fileWatcher?: FileWatcher;
  private hashBaselineStore?: HashBaselineStore;
  private fileDeltaStore?: FileDeltaStore;
  private diffEngine?: DiffEngine;

  // Current baseline tracking (for hash baseline mode)
  private currentBaseline?: HashBaseline;

  constructor(storage: FileCheckpointStorageAdapter, config: OptimizedFileCheckpointManagerConfig) {
    this.storage = storage;
    this.config = { ...DEFAULT_CONFIG, ...config };

    // Initialize optimization components if enabled
    if (this.config.useHashBaseline) {
      this.hashBaselineStore = new HashBaselineStore({
        workspaceRoot: this.config.workspaceRoot,
      });
      this.fileDeltaStore = new FileDeltaStore({
        workspaceRoot: this.config.workspaceRoot,
      });
    }

    if (this.config.enableDiff) {
      this.diffEngine = new DiffEngine();
    }
  }

  /**
   * Initialize the manager
   *
   * If useFileWatcher is enabled, starts watching the workspace.
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    await this.storage.initialize();

    // Start file watcher if enabled
    if (this.config.useFileWatcher) {
      const ignorePatterns = [
        ...HARDCODED_IGNORE_PATTERNS,
        ...(this.config.customIgnorePatterns ?? []),
      ];

      this.fileWatcher = new FileWatcher({
        rootDir: this.config.workspaceRoot,
        ignorePatterns,
        debounceMs: 100,
        ignoreInitial: true,
      });

      try {
        await this.fileWatcher.start();
        logger.info("FileWatcher started", {
          workspaceRoot: this.config.workspaceRoot,
        });
      } catch (err) {
        logger.warn("Failed to start FileWatcher, falling back to scan mode", {
          err,
        });
        this.fileWatcher = undefined;
      }
    }

    this.initialized = true;
    logger.info("FileCheckpointManager initialized", {
      workspaceRoot: this.config.workspaceRoot,
      useFileWatcher: !!this.fileWatcher,
      useHashBaseline: !!this.hashBaselineStore,
    });
  }

  /**
   * Close the manager
   */
  async close(): Promise<void> {
    if (!this.initialized) return;

    if (this.fileWatcher) {
      await this.fileWatcher.stop();
      this.fileWatcher = undefined;
    }

    await this.storage.close();
    this.initialized = false;
    logger.info("FileCheckpointManager closed");
  }

  /**
   * Create a checkpoint
   *
   * If FileWatcher is active and has changes, only processes changed files.
   * Otherwise, performs full workspace scan.
   */
  async createCheckpoint(entityId: string): Promise<FileCheckpointCreateResult> {
    if (!this.initialized) {
      throw new Error("FileCheckpointManager not initialized");
    }

    // Determine if we can use incremental mode
    const useIncremental =
      this.fileWatcher &&
      this.fileWatcher.getIsReady() &&
      this.fileWatcher.getChangedFiles().size > 0;

    if (useIncremental && this.hashBaselineStore && this.currentBaseline) {
      return this.createCheckpointIncremental(entityId);
    }

    // Fall back to full scan mode
    return this.createCheckpointFullScan(entityId);
  }

  /**
   * Create checkpoint using incremental changes from FileWatcher
   */
  private async createCheckpointIncremental(entityId: string): Promise<FileCheckpointCreateResult> {
    const changedFiles = this.fileWatcher!.getChangedFiles();

    logger.debug("Creating incremental checkpoint", {
      entityId,
      changedCount: changedFiles.size,
    });

    // Hash only changed files
    const newHashMap = new Map<string, { hash: string; size: number; modifiedAt: number }>();
    const deltaChanges: Array<{
      path: string;
      type: "added" | "modified" | "deleted";
      oldHash?: string;
      newHash?: string;
      absolutePath?: string;
    }> = [];

    const oldHashMap = this.hashBaselineStore!.getHashMap(this.currentBaseline);

    for (const [absolutePath, changeRecord] of changedFiles) {
      const relativePath = path.relative(this.config.workspaceRoot, absolutePath);

      if (changeRecord.type === "unlink") {
        // File deleted
        const oldHash = oldHashMap.get(relativePath);
        if (oldHash) {
          deltaChanges.push({
            path: relativePath,
            type: "deleted",
            oldHash,
          });
        }
      } else {
        // File added or modified
        try {
          const stats = await fsp.stat(absolutePath);
          const content = await fsp.readFile(absolutePath);
          const hash = createHash("md5").update(content).digest("hex");

          newHashMap.set(relativePath, {
            hash,
            size: stats.size,
            modifiedAt: stats.mtimeMs,
          });

          const oldHash = oldHashMap.get(relativePath);
          deltaChanges.push({
            path: relativePath,
            type: oldHash ? "modified" : "added",
            oldHash,
            newHash: hash,
            absolutePath,
          });
        } catch (err) {
          logger.warn("Failed to process changed file", {
            err,
            path: relativePath,
          });
        }
      }
    }

    // Create delta checkpoint
    const checkpointId = this.generateCheckpointId();
    const timestamp = Date.now();

    // Get base checkpoint ID from current baseline
    const baseCheckpointId = this.currentBaseline?.id ?? checkpointId;

    // Update baseline with new hashes
    const updatedHashMap = new Map(oldHashMap);
    for (const [path, data] of newHashMap) {
      updatedHashMap.set(path, data.hash);
    }
    for (const change of deltaChanges) {
      if (change.type === "deleted") {
        updatedHashMap.delete(change.path);
      }
    }

    // Create delta using FileDeltaStore
    const delta = await this.fileDeltaStore!.createDelta(
      checkpointId,
      baseCheckpointId,
      deltaChanges,
      updatedHashMap.size,
    );

    // Update current baseline reference
    this.currentBaseline = {
      id: checkpointId,
      type: "baseline",
      files: Array.from(updatedHashMap.entries()).map(([relativePath, hash]) => ({
        relativePath,
        hash,
        size: newHashMap.get(relativePath)?.size ?? 0,
        modifiedAt: newHashMap.get(relativePath)?.modifiedAt ?? timestamp,
      })),
      totalFileCount: updatedHashMap.size,
      createdAt: timestamp,
      workspaceRoot: this.config.workspaceRoot,
    };

    // Reset file watcher
    this.fileWatcher!.reset();

    // Save to storage (convert delta to legacy format for backward compatibility)
    const fileContents = new Map<string, Buffer>();
    for (const change of delta.changes) {
      if (change.content) {
        fileContents.set(change.path, change.content);
      }
    }

    const metadata: FileCheckpointMetadata = {
      entityId,
      timestamp,
      type: "incremental",
      baseCheckpointId,
      changes: delta.changes.map(c => ({
        path: c.path,
        type: c.type,
        hash: c.newHash ?? c.oldHash ?? "",
      })),
      fileCount: delta.totalFileCount,
      fileHashSnapshot: Object.fromEntries(updatedHashMap),
      emptyDirs: [],
      totalSize: Array.from(fileContents.values()).reduce((sum, c) => sum + c.length, 0),
      workspaceRoot: this.config.workspaceRoot,
    };

    await this.storage.save(checkpointId, metadata, fileContents);

    logger.info("Incremental checkpoint created", {
      checkpointId,
      entityId,
      changedFiles: deltaChanges.length,
    });

    return { id: checkpointId, metadata };
  }

  /**
   * Create checkpoint using full workspace scan (original behavior)
   */
  private async createCheckpointFullScan(entityId: string): Promise<FileCheckpointCreateResult> {
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
      type: isFullBackup ? "full" : "incremental",
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
          logger.warn("Failed to read file for full backup, skipping", {
            err,
            filePath,
          });
        }
      }
    } else {
      for (const change of changes ?? []) {
        if (change.type === "added" || change.type === "modified") {
          try {
            const content = await this.readFile(change.path);
            fileContents.set(change.path, content);
          } catch (err) {
            logger.warn("Failed to read changed file, skipping", {
              err,
              path: change.path,
            });
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

    // Update hash baseline if enabled
    if (this.hashBaselineStore) {
      this.currentBaseline = await this.hashBaselineStore.createBaselineFromHashMap(
        checkpointId,
        new Map(
          Object.entries(fileHashes).map(([p, h]) => [
            p,
            { hash: h, size: 0, modifiedAt: timestamp },
          ]),
        ),
      );
    }

    logger.info("File checkpoint created", {
      checkpointId,
      entityId,
      type: metadata.type,
      fileCount: metadata.fileCount,
      changedFiles: changes?.length ?? files.length,
      totalSize,
    });

    return { id: checkpointId, metadata };
  }

  /**
   * Restore checkpoint
   */
  async restoreCheckpoint(
    entityId: string,
    checkpointId: string,
  ): Promise<FileCheckpointRestoreResult> {
    if (!this.initialized) {
      throw new Error("FileCheckpointManager not initialized");
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

      try {
        await fsp.access(absolutePath);
        const currentHash = await this.hashFile(absolutePath);
        const targetHash = this.hashContent(content);

        if (currentHash === targetHash) {
          skippedCount++;
        } else {
          filesToRestore.push({ relativePath, content });
        }
      } catch {
        // File doesn't exist
        filesToRestore.push({ relativePath, content });
      }
    }

    for (const relativePath of currentRelativeSet) {
      if (!targetFiles.has(relativePath)) {
        if (!this.shouldIgnore(relativePath)) {
          filesToDelete.push(relativePath);
        }
      }
    }

    // Delete extra files
    for (const relativePath of filesToDelete) {
      try {
        const absolutePath = path.resolve(this.config.workspaceRoot, relativePath);
        await fsp.unlink(absolutePath);
        logger.debug("Deleted extra file during restore", { path: relativePath });
      } catch (err) {
        logger.warn("Failed to delete extra file", { err, path: relativePath });
      }
    }

    // Restore files
    for (const { relativePath, content } of filesToRestore) {
      try {
        const absolutePath = path.resolve(this.config.workspaceRoot, relativePath);
        await fsp.mkdir(path.dirname(absolutePath), { recursive: true });
        await fsp.writeFile(absolutePath, content);
        logger.debug("Restored file", { path: relativePath });
      } catch (err) {
        logger.warn("Failed to restore file", { err, path: relativePath });
      }
    }

    // Restore empty directories
    for (const emptyDir of checkpoint.metadata.emptyDirs) {
      try {
        const absolutePath = path.resolve(this.config.workspaceRoot, emptyDir);
        await fsp.mkdir(absolutePath, { recursive: true });
      } catch (err) {
        logger.warn("Failed to restore empty directory", { err, emptyDir });
      }
    }

    // Reset file watcher after restore
    if (this.fileWatcher) {
      this.fileWatcher.reset();
    }

    logger.info("File checkpoint restored", {
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

  /**
   * Get diff between two file versions
   *
   * Only available if enableDiff is true.
   */
  async getDiff(
    _filePath: string,
    oldContent: string,
    newContent: string,
  ): Promise<DiffResult | null> {
    if (!this.diffEngine) {
      logger.warn("Diff engine not enabled");
      return null;
    }

    return this.diffEngine.diff(oldContent, newContent);
  }

  /**
   * Get unified diff string
   */
  async getUnifiedDiff(filePath: string, oldContent: string, newContent: string): Promise<string> {
    if (!this.diffEngine) {
      return "";
    }

    return this.diffEngine.unifiedDiff(oldContent, newContent, filePath, filePath);
  }

  /**
   * Get storage adapter
   */
  getStorage(): FileCheckpointStorageAdapter {
    return this.storage;
  }

  /**
   * Get file watcher (if enabled)
   */
  getFileWatcher(): FileWatcher | undefined {
    return this.fileWatcher;
  }

  /**
   * Manually notify file change (for external events)
   */
  notifyFileChange(filePath: string, type: "add" | "change" | "unlink"): void {
    if (this.fileWatcher) {
      this.fileWatcher.addChangeRecord({
        path: filePath,
        type,
        timestamp: Date.now(),
      });
    }
  }

  // ============================================================================
  // Private helper methods (same as original implementation)
  // ============================================================================

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
      entries = await fsp.readdir(currentDir, { withFileTypes: true });
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
        await this.scanDirectory(
          rootDir,
          path.resolve(currentDir, entry.name),
          ignorePatterns,
          files,
          allDirs,
        );
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

  private async collectGitignoreFiles(
    rootDir: string,
    currentDir: string,
    patterns: string[],
  ): Promise<void> {
    const gitignorePath = path.join(currentDir, ".gitignore");

    try {
      const content = await fsp.readFile(gitignorePath, "utf-8");
      const relativePrefix = path.relative(rootDir, currentDir);

      for (const line of content.split("\n")) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith("#")) continue;

        const prefixed =
          relativePrefix === ""
            ? trimmed
            : path.posix.join(relativePrefix.replace(/\\/g, "/"), trimmed);

        patterns.push(prefixed);
      }
    } catch {
      // File not found or permission error, skip
    }

    let entries: fs.Dirent[];
    try {
      entries = await fsp.readdir(currentDir, { withFileTypes: true });
    } catch {
      return;
    }

    for (const entry of entries) {
      if (entry.isDirectory() && entry.name !== ".git" && entry.name !== "node_modules") {
        await this.collectGitignoreFiles(rootDir, path.join(currentDir, entry.name), patterns);
      }
    }
  }

  private isIgnored(relativePath: string, entryName: string, patterns: string[]): boolean {
    const normalizedPath = relativePath.replace(/\\/g, "/");

    for (const pattern of patterns) {
      const normalizedPattern = pattern.replace(/\\/g, "/");

      if (normalizedPattern.endsWith("/")) {
        const prefix = normalizedPattern.slice(0, -1);
        if (normalizedPath.startsWith(prefix) || normalizedPath === prefix) {
          return true;
        }
      }

      if (
        normalizedPath === normalizedPattern ||
        normalizedPath.startsWith(normalizedPattern + "/")
      ) {
        return true;
      }

      if (entryName === normalizedPattern) {
        return true;
      }

      if (normalizedPattern.includes("*")) {
        const regex = new RegExp(
          "^" + normalizedPattern.replace(/\*/g, "[^/]*").replace(/\?/g, "[^/]") + "$",
        );
        if (regex.test(normalizedPath) || regex.test(entryName)) {
          return true;
        }
      }
    }

    return false;
  }

  private shouldIgnore(relativePath: string): boolean {
    return this.isIgnored(relativePath, path.basename(relativePath), HARDCODED_IGNORE_PATTERNS);
  }

  private async computeFileHashes(files: string[]): Promise<Record<string, string>> {
    const result: Record<string, string> = {};

    for (const filePath of files) {
      try {
        const absolutePath = path.resolve(this.config.workspaceRoot, filePath);
        result[filePath] = await this.hashFile(absolutePath);
      } catch (err) {
        logger.warn("Failed to hash file, skipping", { err, filePath });
      }
    }

    return result;
  }

  private async hashFile(absolutePath: string): Promise<string> {
    const content = await fsp.readFile(absolutePath);
    return this.hashContent(content);
  }

  private hashContent(content: Buffer): string {
    return createHash("md5").update(content).digest("hex");
  }

  private computeChanges(
    oldHashes: Record<string, string>,
    newHashes: Record<string, string>,
  ): FileChangeRecord[] {
    const changes: FileChangeRecord[] = [];

    for (const [filePath, newHash] of Object.entries(newHashes)) {
      const oldHash = oldHashes[filePath];

      if (!oldHash) {
        changes.push({ path: filePath, type: "added", hash: newHash });
      } else if (oldHash !== newHash) {
        changes.push({ path: filePath, type: "modified", hash: newHash });
      }
    }

    for (const [filePath, oldHash] of Object.entries(oldHashes)) {
      if (!(filePath in newHashes)) {
        changes.push({ path: filePath, type: "deleted", hash: oldHash });
      }
    }

    return changes;
  }

  private findEmptyDirs(allDirs: string[], files: string[]): string[] {
    const fileParentDirs = new Set<string>();

    for (const filePath of files) {
      let dir = path.dirname(filePath);
      while (dir !== ".") {
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
      if (cp.metadata.type === "incremental") {
        length++;
      }
    }

    return length;
  }

  private async resolveTargetFiles(checkpointId: string): Promise<Map<string, Buffer>> {
    const targetFiles = new Map<string, Buffer>();
    const visited = new Set<string>();

    const chain: Array<{
      id: string;
      metadata: FileCheckpointMetadata;
    }> = [];
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

      if (link.metadata.type === "full") {
        for (const [relativePath, content] of checkpoint.files) {
          targetFiles.set(relativePath, content);
        }
      } else {
        for (const change of link.metadata.changes ?? []) {
          if (change.type === "deleted") {
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
    return fsp.readFile(absolutePath);
  }

  private generateCheckpointId(): string {
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substring(2, 10);
    return `${timestamp}-${random}`;
  }
}
