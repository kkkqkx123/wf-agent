/**
 * File Watcher - Chokidar-based incremental file change detection
 *
 * Replaces full workspace scan with persistent file watching.
 * Tracks changed files incrementally for efficient checkpoint creation.
 */

import { FSWatcher, watch } from "chokidar";
import { EventEmitter } from "events";
import * as path from "path";

/**
 * File change event types
 */
export type FileChangeType = "add" | "change" | "unlink";

/**
 * File change record
 */
export interface FileChangeRecord {
  path: string;
  type: FileChangeType;
  timestamp: number;
}

/**
 * File watcher configuration
 */
export interface FileWatcherConfig {
  /** Root directory to watch */
  rootDir: string;
  /** Ignore patterns (glob) */
  ignorePatterns?: string[];
  /** Debounce interval in milliseconds (default: 100) */
  debounceMs?: number;
  /** Whether to ignore initial add events (default: true) */
  ignoreInitial?: boolean;
  /** Whether to follow symlinks (default: false) */
  followSymlinks?: boolean;
}

/**
 * File watcher events
 */
export interface FileWatcherEvents {
  change: (record: FileChangeRecord) => void;
  error: (error: Error) => void;
  ready: () => void;
}

/**
 * Incremental file watcher using chokidar
 *
 * Tracks file changes in real-time, allowing checkpoint creation
 * to only process actually changed files instead of scanning entire workspace.
 *
 * @example
 * ```typescript
 * const watcher = new FileWatcher({
 *   rootDir: '/path/to/workspace',
 *   ignorePatterns: ['node_modules', '.git', 'dist'],
 * });
 *
 * await watcher.start();
 *
 * // Get changed files since last reset
 * const changes = watcher.getChangedFiles();
 *
 * // Reset after checkpoint created
 * watcher.reset();
 * ```
 */
export class FileWatcher extends EventEmitter {
  private readonly config: Required<FileWatcherConfig>;
  private watcher?: FSWatcher;
  private changedFiles: Map<string, FileChangeRecord> = new Map();
  private pendingChanges: Map<string, FileChangeRecord> = new Map();
  private debounceTimer?: NodeJS.Timeout;
  private isReady = false;

  constructor(config: FileWatcherConfig) {
    super();
    this.config = {
      rootDir: config.rootDir,
      ignorePatterns: config.ignorePatterns ?? [],
      debounceMs: config.debounceMs ?? 100,
      ignoreInitial: config.ignoreInitial ?? true,
      followSymlinks: config.followSymlinks ?? false,
    };
  }

  /**
   * Start watching the directory
   */
  async start(): Promise<void> {
    if (this.watcher) {
      throw new Error("FileWatcher is already running");
    }

    const ignored = this.buildIgnorePatterns();

    this.watcher = watch(this.config.rootDir, {
      ignored,
      ignoreInitial: this.config.ignoreInitial,
      followSymlinks: this.config.followSymlinks,
      persistent: true,
      usePolling: false,
      awaitWriteFinish: {
        stabilityThreshold: 100,
        pollInterval: 50,
      },
    });

    this.setupEventHandlers();

    return new Promise((resolve, reject) => {
      this.watcher!.on("ready", () => {
        this.isReady = true;
        this.emit("ready");
        resolve();
      });

      this.watcher!.on("error", (error) => {
        this.emit("error", error instanceof Error ? error : new Error(String(error)));
        reject(error);
      });
    });
  }

  /**
   * Stop watching
   */
  async stop(): Promise<void> {
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
      this.debounceTimer = undefined;
    }

    if (this.watcher) {
      await this.watcher.close();
      this.watcher = undefined;
    }

    this.isReady = false;
    this.pendingChanges.clear();
  }

  /**
   * Get all changed files since last reset
   */
  getChangedFiles(): Map<string, FileChangeRecord> {
    return new Map(this.changedFiles);
  }

  /**
   * Get changed file paths only
   */
  getChangedPaths(): string[] {
    return Array.from(this.changedFiles.keys());
  }

  /**
   * Check if a specific file has changed
   */
  hasChanged(filePath: string): boolean {
    const absolutePath = path.isAbsolute(filePath)
      ? filePath
      : path.join(this.config.rootDir, filePath);
    return this.changedFiles.has(absolutePath);
  }

  /**
   * Reset change tracking (call after checkpoint is created)
   */
  reset(): void {
    this.changedFiles.clear();
    this.pendingChanges.clear();
  }

  /**
   * Check if watcher is ready
   */
  getIsReady(): boolean {
    return this.isReady;
  }

  /**
   * Get current watched directory
   */
  getRootDir(): string {
    return this.config.rootDir;
  }

  /**
   * Manually add a file change record (for external events)
   */
  addChangeRecord(record: FileChangeRecord): void {
    const absolutePath = path.isAbsolute(record.path)
      ? record.path
      : path.join(this.config.rootDir, record.path);

    this.changedFiles.set(absolutePath, {
      ...record,
      path: absolutePath,
    });
  }

  /**
   * Build chokidar ignore patterns
   */
  private buildIgnorePatterns(): (string | RegExp)[] {
    const patterns: (string | RegExp)[] = [];

    for (const pattern of this.config.ignorePatterns) {
      // Convert glob pattern to RegExp for directories
      if (pattern.endsWith("/")) {
        // Directory pattern: node_modules/ -> matches any node_modules directory
        const dirName = pattern.slice(0, -1);
        patterns.push(new RegExp(`(^|/)${this.escapeRegExp(dirName)}(/|$)`));
      } else if (pattern.startsWith("*")) {
        // Glob pattern: *.log -> convert to RegExp
        const regexPattern = pattern
          .replace(/\./g, "\\.")
          .replace(/\*/g, "[^/]*");
        patterns.push(new RegExp(regexPattern + "$"));
      } else {
        // Exact match
        patterns.push(pattern);
      }
    }

    return patterns;
  }

  /**
   * Escape special regex characters
   */
  private escapeRegExp(str: string): string {
    return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }

  /**
   * Setup chokidar event handlers
   */
  private setupEventHandlers(): void {
    if (!this.watcher) return;

    const handleChange = (type: FileChangeType) => (filePath: string) => {
      const record: FileChangeRecord = {
        path: filePath,
        type,
        timestamp: Date.now(),
      };

      this.pendingChanges.set(filePath, record);
      this.scheduleDebounce();
    };

    this.watcher.on("add", handleChange("add"));
    this.watcher.on("change", handleChange("change"));
    this.watcher.on("unlink", handleChange("unlink"));
  }

  /**
   * Schedule debounced flush of pending changes
   */
  private scheduleDebounce(): void {
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
    }

    this.debounceTimer = setTimeout(() => {
      this.flushPendingChanges();
    }, this.config.debounceMs);
  }

  /**
   * Flush pending changes to main change map
   */
  private flushPendingChanges(): void {
    for (const [filePath, record] of this.pendingChanges) {
      // Handle unlink: remove from changedFiles if previously added/changed
      if (record.type === "unlink") {
        const existing = this.changedFiles.get(filePath);
        if (existing && (existing.type === "add" || existing.type === "change")) {
          // File was added then deleted - remove from tracking
          this.changedFiles.delete(filePath);
        } else {
          // File was deleted - track as unlink
          this.changedFiles.set(filePath, record);
        }
      } else {
        this.changedFiles.set(filePath, record);
      }

      this.emit("change", record);
    }

    this.pendingChanges.clear();
    this.debounceTimer = undefined;
  }

  /**
   * Event emitter type-safe emit
   */
  override emit<K extends keyof FileWatcherEvents>(
    event: K,
    ...args: Parameters<FileWatcherEvents[K]>
  ): boolean {
    return super.emit(event, ...args);
  }

  /**
   * Event emitter type-safe on
   */
  override on<K extends keyof FileWatcherEvents>(
    event: K,
    listener: FileWatcherEvents[K],
  ): this {
    return super.on(event, listener);
  }

  /**
   * Event emitter type-safe off
   */
  override off<K extends keyof FileWatcherEvents>(
    event: K,
    listener: FileWatcherEvents[K],
  ): this {
    return super.off(event, listener);
  }
}
