/**
 * Hash Baseline Store
 *
 * Stores only file hashes for baseline checkpoints.
 * This dramatically reduces initial storage compared to storing full file content.
 *
 * Baseline: Map<path, hash> - only 32 bytes per file
 * vs. Full content: potentially MB per file
 */

import * as crypto from "crypto";
import * as fs from "fs/promises";

/**
 * File hash record
 */
export interface FileHashRecord {
  /** Relative path from workspace root */
  relativePath: string;
  /** MD5 hash of file content */
  hash: string;
  /** File size in bytes */
  size: number;
  /** Last modified timestamp */
  modifiedAt: number;
}

/**
 * Baseline checkpoint data
 */
export interface HashBaseline {
  /** Checkpoint ID */
  id: string;
  /** Type marker */
  type: "baseline";
  /** File hash records */
  files: FileHashRecord[];
  /** Total file count */
  totalFileCount: number;
  /** Creation timestamp */
  createdAt: number;
  /** Workspace root directory (for reference) */
  workspaceRoot: string;
}

/**
 * Hash baseline store configuration
 */
export interface HashBaselineStoreConfig {
  /** Workspace root directory */
  workspaceRoot: string;
  /** Storage adapter for persistence */
  storageAdapter?: HashBaselineStorageAdapter;
}

/**
 * Storage adapter interface for hash baseline persistence
 */
export interface HashBaselineStorageAdapter {
  /** Save baseline */
  saveBaseline(baseline: HashBaseline): Promise<void>;
  /** Load baseline by ID */
  loadBaseline(id: string): Promise<HashBaseline | null>;
  /** Load latest baseline */
  loadLatestBaseline(): Promise<HashBaseline | null>;
  /** Delete baseline */
  deleteBaseline(id: string): Promise<void>;
  /** List all baseline IDs */
  listBaselineIds(): Promise<string[]>;
}

/**
 * Hash Baseline Store
 *
 * Manages baseline checkpoints containing only file hashes.
 */
export class HashBaselineStore {
  private readonly config: HashBaselineStoreConfig;
  private currentBaseline?: HashBaseline;

  constructor(config: HashBaselineStoreConfig) {
    this.config = config;
  }

  /**
   * Create a baseline from current workspace state
   */
  async createBaseline(
    id: string,
    files: Array<{ relativePath: string; absolutePath: string }>,
  ): Promise<HashBaseline> {
    const fileRecords: FileHashRecord[] = [];

    for (const file of files) {
      try {
        const stats = await fs.stat(file.absolutePath);
        const hash = await this.computeFileHash(file.absolutePath);

        fileRecords.push({
          relativePath: file.relativePath,
          hash,
          size: stats.size,
          modifiedAt: stats.mtimeMs,
        });
      } catch {
        // Skip files that can't be read
        continue;
      }
    }

    const baseline: HashBaseline = {
      id,
      type: "baseline",
      files: fileRecords,
      totalFileCount: fileRecords.length,
      createdAt: Date.now(),
      workspaceRoot: this.config.workspaceRoot,
    };

    if (this.config.storageAdapter) {
      await this.config.storageAdapter.saveBaseline(baseline);
    }

    this.currentBaseline = baseline;
    return baseline;
  }

  /**
   * Create baseline from existing hash map (no file I/O)
   */
  async createBaselineFromHashMap(
    id: string,
    hashMap: Map<string, { hash: string; size: number; modifiedAt: number }>,
  ): Promise<HashBaseline> {
    const fileRecords: FileHashRecord[] = [];

    for (const [relativePath, data] of hashMap) {
      fileRecords.push({
        relativePath,
        hash: data.hash,
        size: data.size,
        modifiedAt: data.modifiedAt,
      });
    }

    const baseline: HashBaseline = {
      id,
      type: "baseline",
      files: fileRecords,
      totalFileCount: fileRecords.length,
      createdAt: Date.now(),
      workspaceRoot: this.config.workspaceRoot,
    };

    if (this.config.storageAdapter) {
      await this.config.storageAdapter.saveBaseline(baseline);
    }

    this.currentBaseline = baseline;
    return baseline;
  }

  /**
   * Load baseline by ID
   */
  async loadBaseline(id: string): Promise<HashBaseline | null> {
    if (this.config.storageAdapter) {
      const baseline = await this.config.storageAdapter.loadBaseline(id);
      if (baseline) {
        this.currentBaseline = baseline;
      }
      return baseline;
    }
    return null;
  }

  /**
   * Load latest baseline
   */
  async loadLatestBaseline(): Promise<HashBaseline | null> {
    if (this.config.storageAdapter) {
      const baseline = await this.config.storageAdapter.loadLatestBaseline();
      if (baseline) {
        this.currentBaseline = baseline;
      }
      return baseline;
    }
    return null;
  }

  /**
   * Get current baseline (in-memory)
   */
  getCurrentBaseline(): HashBaseline | undefined {
    return this.currentBaseline;
  }

  /**
   * Set current baseline (in-memory)
   */
  setCurrentBaseline(baseline: HashBaseline): void {
    this.currentBaseline = baseline;
  }

  /**
   * Get file hash map from baseline
   */
  getHashMap(baseline?: HashBaseline): Map<string, string> {
    const target = baseline ?? this.currentBaseline;
    if (!target) {
      return new Map();
    }

    const hashMap = new Map<string, string>();
    for (const file of target.files) {
      hashMap.set(file.relativePath, file.hash);
    }
    return hashMap;
  }

  /**
   * Get file records map from baseline
   */
  getFileRecordsMap(baseline?: HashBaseline): Map<string, FileHashRecord> {
    const target = baseline ?? this.currentBaseline;
    if (!target) {
      return new Map();
    }

    const recordsMap = new Map<string, FileHashRecord>();
    for (const file of target.files) {
      recordsMap.set(file.relativePath, file);
    }
    return recordsMap;
  }

  /**
   * Compare two baselines and return changed files
   */
  compareBaselines(
    oldBaseline: HashBaseline,
    newBaseline: HashBaseline,
  ): {
    added: FileHashRecord[];
    modified: Array<{ old: FileHashRecord; new: FileHashRecord }>;
    deleted: FileHashRecord[];
    unchanged: FileHashRecord[];
  } {
    const oldMap = new Map<string, FileHashRecord>();
    for (const file of oldBaseline.files) {
      oldMap.set(file.relativePath, file);
    }

    const newMap = new Map<string, FileHashRecord>();
    for (const file of newBaseline.files) {
      newMap.set(file.relativePath, file);
    }

    const added: FileHashRecord[] = [];
    const modified: Array<{ old: FileHashRecord; new: FileHashRecord }> = [];
    const deleted: FileHashRecord[] = [];
    const unchanged: FileHashRecord[] = [];

    // Check new files
    for (const [path, newRecord] of newMap) {
      const oldRecord = oldMap.get(path);
      if (!oldRecord) {
        added.push(newRecord);
      } else if (oldRecord.hash !== newRecord.hash) {
        modified.push({ old: oldRecord, new: newRecord });
      } else {
        unchanged.push(newRecord);
      }
    }

    // Check deleted files
    for (const [path, oldRecord] of oldMap) {
      if (!newMap.has(path)) {
        deleted.push(oldRecord);
      }
    }

    return { added, modified, deleted, unchanged };
  }

  /**
   * Compute MD5 hash of a file
   */
  private async computeFileHash(filePath: string): Promise<string> {
    const content = await fs.readFile(filePath);
    return crypto.createHash("md5").update(content).digest("hex");
  }
}
